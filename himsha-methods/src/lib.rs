//! Generated guest constants. `risc0-build` emits `HIMSHA_GUEST_ELF` (the RISC-V
//! ELF) and `HIMSHA_GUEST_ID` (its image id) at build time.
include!(concat!(env!("OUT_DIR"), "/methods.rs"));
