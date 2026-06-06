fn main() {
    // Compiles the `guest` crate to a RISC-V ELF and emits `HIMSHA_GUEST_ELF` /
    // `HIMSHA_GUEST_ID` into OUT_DIR/methods.rs. Requires the RISC Zero toolchain.
    risc0_build::embed_methods();
}
