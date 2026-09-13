use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=IW4L_UPDATER_CA_CERT");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let ca_out = out.join("iw4l-ca.pem");
    match std::env::var_os("IW4L_UPDATER_CA_CERT") {
        Some(path) => {
            std::fs::copy(&path, &ca_out).unwrap_or_else(|error| {
                panic!(
                    "copy updater CA from {}: {error}",
                    PathBuf::from(path).display()
                )
            });
        }
        None => std::fs::write(
            &ca_out,
            b"IW4L updater built without IW4L_UPDATER_CA_CERT; use `make launcher windows`\n",
        )
        .expect("write explicit missing-CA marker"),
    };
}
