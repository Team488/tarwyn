fn main() -> Result<(), Box<dyn std::error::Error>> {
    uniffi_bindgen_java::run_main()?;
    Ok(())
}
