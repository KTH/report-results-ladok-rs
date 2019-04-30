use ructe::{Ructe, RucteError};

fn main() -> Result<(), RucteError> {
    let mut ructe = Ructe::from_env()?;
    let mut statics = ructe.statics()?;
    statics.add_sass_file("style/simple.scss")?;
    ructe.compile_templates("templates")?;
    Ok(())
}
