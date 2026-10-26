//! Dragging sampler items out to the Finder as file promises (macOS): the
//! drag names the files but holds no data. When they are dropped, the Finder
//! says where; the frontend then queues ordinary transfers into that folder
//! and calls `drag_out_finish`, which tells the Finder the files are there.
//! Nothing is fetched for a drag that is cancelled.
//!
//! Events: `drag-out-drop` `{ token, folder }` once per drag, when the first
//! file is asked for; `drag-out-end` `{ token, dropped }` when the drag ends.
//!
//! Other systems keep the fetch-first drag (`tauri-plugin-drag`).

use serde::Serialize;

use crate::transfer::{Source, file_name};
use crate::worker::CmdResult;

#[derive(Clone, Serialize)]
pub struct DropEvent {
    pub token: u32,
    pub folder: std::path::PathBuf,
}

#[derive(Clone, Serialize)]
pub struct EndEvent {
    pub token: u32,
    pub dropped: bool,
}

/// The file name each item is written under, as the transfers will write it.
fn names(items: &[Source]) -> CmdResult<Vec<String>> {
    items
        .iter()
        .map(|s| {
            let (_, kind, name) = s.split()?;
            Ok(file_name(&name, kind.ext()))
        })
        .collect()
}

/// Starts a promise drag of `items` at the pointer; `icon` is a PNG.
#[tauri::command]
pub fn drag_out_start(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    items: Vec<Source>,
    icon: Vec<u8>,
) -> CmdResult<u32> {
    let names = names(&items)?;
    if names.is_empty() {
        return Err("nothing to drag".into());
    }
    imp::start(app, &window, names, icon)
}

/// The transfers for drag `token` are done (`error` = None) or failed.
#[tauri::command]
pub fn drag_out_finish(token: u32, error: Option<String>) {
    imp::finish(token, error);
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use crate::worker::CmdResult;

    pub fn start(
        _app: tauri::AppHandle,
        _window: &tauri::WebviewWindow,
        _names: Vec<String>,
        _icon: Vec<u8>,
    ) -> CmdResult<u32> {
        Err("file promise drags are macOS only".into())
    }

    pub fn finish(_token: u32, _error: Option<String>) {}
}

#[cfg(target_os = "macos")]
mod imp {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};

    use block2::{DynBlock, RcBlock};
    use objc2::rc::Retained;
    use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
    use objc2_app_kit::{
        NSApp, NSDragOperation, NSDraggingContext, NSDraggingItem, NSDraggingSession, NSDraggingSource,
        NSEvent, NSEventModifierFlags, NSEventType, NSFilePromiseProvider, NSFilePromiseProviderDelegate,
        NSImage, NSView,
    };
    use objc2_foundation::{
        NSCocoaErrorDomain, NSData, NSError, NSMutableArray, NSPoint, NSRect, NSString, NSURL,
    };
    use tauri::Emitter;

    use super::{DropEvent, EndEvent};
    use crate::worker::CmdResult;

    /// `NSUserCancelledError`: the Finder drops the promise without a message.
    const USER_CANCELLED: isize = 3072;
    /// `NSFileWriteUnknownError`.
    const WRITE_FAILED: isize = 512;

    /// A completion handler from the Finder. Blocks may be called from any
    /// thread; they are only stored here and called once.
    struct Completion(RcBlock<dyn Fn(*mut NSError)>);
    unsafe impl Send for Completion {}

    #[derive(Default)]
    struct Session {
        /// The drop folder has been sent to the frontend.
        announced: bool,
        /// Set by `finish`: None = success, Some(message) = failure.
        result: Option<Option<String>>,
        /// Handlers waiting for `finish`.
        waiting: Vec<Completion>,
    }

    static SESSIONS: Mutex<Option<HashMap<u32, Session>>> = Mutex::new(None);
    static NEXT: AtomicU32 = AtomicU32::new(1);

    thread_local! {
        /// The promise providers hold their delegates weakly; keep them alive
        /// until the drag's transfers finish. Main thread only.
        static DELEGATES: RefCell<HashMap<u32, Vec<Retained<PromiseDelegate>>>> = RefCell::default();
    }

    fn with_sessions<R>(f: impl FnOnce(&mut HashMap<u32, Session>) -> R) -> R {
        let mut guard = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
        f(guard.get_or_insert_with(HashMap::new))
    }

    fn complete(handler: &Completion, result: &Option<String>, cancelled: bool) {
        match result {
            None if !cancelled => handler.0.call((std::ptr::null_mut(),)),
            _ => {
                let code = if cancelled { USER_CANCELLED } else { WRITE_FAILED };
                let domain = unsafe { NSCocoaErrorDomain };
                let error = NSError::new(code, domain);
                handler.0.call((Retained::as_ptr(&error) as *mut NSError,));
            }
        }
    }

    struct DelegateIvars {
        token: u32,
        name: String,
        app: tauri::AppHandle,
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "S5kPromiseDelegate"]
        #[ivars = DelegateIvars]
        struct PromiseDelegate;

        unsafe impl NSObjectProtocol for PromiseDelegate {}

        unsafe impl NSFilePromiseProviderDelegate for PromiseDelegate {
            #[unsafe(method_id(filePromiseProvider:fileNameForType:))]
            fn file_name(
                &self,
                _provider: &NSFilePromiseProvider,
                _file_type: &NSString,
            ) -> Retained<NSString> {
                NSString::from_str(&self.ivars().name)
            }

            // Called on the main queue: record the request and return; the
            // handler is called when the frontend's transfers finish.
            #[unsafe(method(filePromiseProvider:writePromiseToURL:completionHandler:))]
            fn write_promise(
                &self,
                _provider: &NSFilePromiseProvider,
                url: &NSURL,
                completion: &DynBlock<dyn Fn(*mut NSError)>,
            ) {
                let ivars = self.ivars();
                let handler = Completion(completion.copy());
                let folder: Option<PathBuf> = url
                    .path()
                    .map(|p| PathBuf::from(p.to_string()))
                    .and_then(|p| p.parent().map(PathBuf::from));
                let announce = with_sessions(|sessions| {
                    let s = sessions.entry(ivars.token).or_default();
                    if let Some(result) = &s.result {
                        complete(&handler, result, false);
                        return None;
                    }
                    s.waiting.push(handler);
                    if s.announced {
                        return None;
                    }
                    s.announced = true;
                    folder.clone()
                });
                match announce {
                    Some(folder) => {
                        let _ = ivars.app.emit(
                            "drag-out-drop",
                            DropEvent {
                                token: ivars.token,
                                folder,
                            },
                        );
                    }
                    None if folder.is_none() => finish(ivars.token, Some("no destination".into())),
                    None => {}
                }
            }
        }
    );

    impl PromiseDelegate {
        fn new(token: u32, name: String, app: tauri::AppHandle, mtm: MainThreadMarker) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(DelegateIvars { token, name, app });
            unsafe { msg_send![super(this), init] }
        }
    }

    struct SourceIvars {
        token: u32,
        app: tauri::AppHandle,
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "S5kDragOutSource"]
        #[ivars = SourceIvars]
        struct DragSource;

        unsafe impl NSObjectProtocol for DragSource {}

        unsafe impl NSDraggingSource for DragSource {
            #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
            fn operation_mask(
                &self,
                _session: &NSDraggingSession,
                _context: NSDraggingContext,
            ) -> NSDragOperation {
                NSDragOperation::Copy
            }

            #[unsafe(method(draggingSession:endedAtPoint:operation:))]
            fn ended(&self, _session: &NSDraggingSession, _at: NSPoint, operation: NSDragOperation) {
                let ivars = self.ivars();
                let dropped = operation != NSDragOperation::None;
                if !dropped {
                    // No promise will be asked for.
                    DELEGATES.with(|d| d.borrow_mut().remove(&ivars.token));
                    with_sessions(|s| s.remove(&ivars.token));
                }
                let _ = ivars.app.emit(
                    "drag-out-end",
                    EndEvent {
                        token: ivars.token,
                        dropped,
                    },
                );
            }
        }
    );

    impl DragSource {
        fn new(token: u32, app: tauri::AppHandle, mtm: MainThreadMarker) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(SourceIvars { token, app });
            unsafe { msg_send![super(this), init] }
        }
    }

    pub fn start(
        app: tauri::AppHandle,
        window: &tauri::WebviewWindow,
        names: Vec<String>,
        icon: Vec<u8>,
    ) -> CmdResult<u32> {
        let mtm = MainThreadMarker::new().ok_or("drag must start on the main thread")?;
        let ns_view = window.ns_view().map_err(|e| e.to_string())?;
        let view = unsafe { &*(ns_view as *const NSView) };
        let ns_window = view.window().ok_or("no window")?;
        let content = ns_window.contentView().ok_or("no content view")?;

        let token = NEXT.fetch_add(1, Ordering::Relaxed);
        with_sessions(|s| s.insert(token, Session::default()));

        let image =
            NSImage::initWithData(NSImage::alloc(), &NSData::from_vec(icon)).ok_or("bad drag image")?;
        let at = ns_window.mouseLocationOutsideOfEventStream();
        let size = image.size();
        let frame = NSRect::new(
            NSPoint::new(at.x - size.width / 2.0, at.y - size.height / 2.0),
            size,
        );

        let items = NSMutableArray::new();
        let mut delegates = Vec::new();
        for (i, name) in names.into_iter().enumerate() {
            let delegate = PromiseDelegate::new(token, name, app.clone(), mtm);
            let provider = NSFilePromiseProvider::initWithFileType_delegate(
                NSFilePromiseProvider::alloc(),
                &NSString::from_str("public.data"),
                ProtocolObject::from_ref(&*delegate),
            );
            let item = NSDraggingItem::initWithPasteboardWriter(
                NSDraggingItem::alloc(),
                ProtocolObject::from_ref(&*provider),
            );
            // Stack several files slightly, like the Finder.
            let offset = (i.min(4) as f64) * 3.0;
            let f = NSRect::new(
                NSPoint::new(frame.origin.x + offset, frame.origin.y - offset),
                frame.size,
            );
            unsafe { item.setDraggingFrame_contents(f, Some(&*image)) };
            items.addObject(&*item);
            delegates.push(delegate);
        }
        DELEGATES.with(|d| d.borrow_mut().insert(token, delegates));

        // As in drag-rs: a drag event at the pointer, timed like the current one.
        let timestamp = NSApp(mtm).currentEvent().map(|e| e.timestamp()).unwrap_or(0.0);
        let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            NSEventType::LeftMouseDragged,
            at,
            NSEventModifierFlags::empty(),
            timestamp,
            ns_window.windowNumber(),
            None,
            0,
            1,
            1.0,
        )
        .ok_or("cannot create the drag event")?;
        let source = DragSource::new(token, app, mtm);
        content.beginDraggingSessionWithItems_event_source(
            &items,
            &event,
            ProtocolObject::from_ref(&*source),
        );
        Ok(token)
    }

    pub fn finish(token: u32, error: Option<String>) {
        let cancelled = error.as_deref() == Some("cancelled");
        let waiting = with_sessions(|sessions| {
            let s = sessions.entry(token).or_default();
            s.result = Some(error.clone());
            std::mem::take(&mut s.waiting)
        });
        for handler in &waiting {
            complete(handler, &error, cancelled);
        }
        // Delegates are dropped on the main thread; `finish` is a sync
        // command, which Tauri runs there.
        if MainThreadMarker::new().is_some() {
            DELEGATES.with(|d| d.borrow_mut().remove(&token));
        }
    }
}
