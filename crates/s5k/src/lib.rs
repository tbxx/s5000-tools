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
pub mod params;
pub mod sampler;
pub mod sim;
pub mod sim_disk;

pub use bundle::{Bundle, ItemRef, ProgramDeps};
pub use error::{Error, Result};
pub use params::{Target, Value, catalog};
pub use s5k_protocol::meters::Meters;
pub use s5k_protocol::panel::{Key, Screen};
/// Screen dimensions in pixels.
pub mod panel_size {
    pub use s5k_protocol::panel::{HEIGHT, WIDTH};
}
pub use s5k_protocol::{ItemKind, Transport};
pub use sampler::{MemoryList, MultiInfo, ProgramInfo, SampleInfo, Sampler, SamplerInfo};
pub use sim::Simulator;
