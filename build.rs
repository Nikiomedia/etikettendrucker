// Bettet das App-Symbol und die Versionsinfos in die .exe ein.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Etikettendrucker");
        res.set("FileDescription", "Etikettendrucker - Pristar/Marklife P15");
        res.set("CompanyName", "Etikettendrucker");
        if let Err(e) = res.compile() {
            println!("cargo:warning=App-Symbol konnte nicht eingebettet werden: {e}");
        }
    }
}
