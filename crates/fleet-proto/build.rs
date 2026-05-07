use std::io::Result;

fn main() -> Result<()> {
    let proto = "../../proto/fleet.proto";
    println!("cargo:rerun-if-changed={proto}");
    prost_build::Config::new()
        .compile_protos(&[proto], &["../../proto"])?;
    Ok(())
}
