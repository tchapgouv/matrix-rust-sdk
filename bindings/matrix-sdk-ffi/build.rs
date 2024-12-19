use std::error::Error;
use vergen::EmitBuilder;

fn main() -> Result<(), Box<dyn Error>> {
    uniffi::generate_scaffolding("./src/api.udl").expect("Building the UDL file failed");
    EmitBuilder::builder().git_sha(true).emit()?;
    Ok(())
}
