use std::path::PathBuf;
use std::process::Command;
use std::{env, io};
#[cfg(windows)]
use winres::WindowsResource;

fn build_startup_stub() -> io::Result<()> {
    let output = PathBuf::from(env::var("OUT_DIR").unwrap()).join("startup_stub.exe");
    let status = Command::new(env::var("RUSTC").unwrap())
        .args(["--edition=2021", "-C", "opt-level=s", "-C", "strip=symbols", "stub/startup.rs", "-o"])
        .arg(output)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("no se pudo compilar stub/startup.rs"))
    }
}

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=stub/startup.rs");
    println!("cargo:rerun-if-changed=icon.ico");
    build_startup_stub()?;

    #[cfg(windows)]
    {
        let mut res = WindowsResource::new();
        res.set_icon("icon.ico");
        res.set_manifest(
            r#"
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
<trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
        <requestedPrivileges>
            <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
        </requestedPrivileges>
    </security>
</trustInfo>
</assembly>
"#,
        );
        res.compile()?;
    }
    Ok(())
}
