//! Control an Akai S5000/S6000 over any [`Transport`]: list and manage
//! memory, transfer samples/programs/multis (with their dependencies), edit
//! parameters, use disks, the front panel and the meters.
//!
//! ```no_run
//! # fn demo<T: s5k::Transport>(transport: T) -> s5k::Result<()> {
//! let mut sampler = s5k::Sampler::connect(transport)?;
//! for p in sampler.memory()?.programs {
//!     println!("{} ({} keygroups)", p.name, p.keygroups);
//! }
//! # Ok(()) }
//! ```

pub mod bundle;
pub mod disk;
pub mod error;
pub mod sampler;

pub use bundle::Bundle;
pub use error::{Error, Result};
pub use s5k_protocol::{ItemKind, Transport, panel::Key};
pub use sampler::{MemoryList, MultiInfo, ProgramInfo, SampleInfo, Sampler, SamplerInfo};
