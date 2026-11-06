// Intègre l'icône et les informations « Live Régie » dans les exe Windows
// (icône visible dans l'Explorateur, la barre des tâches et les raccourcis)
fn main() {
    println!("cargo:rerun-if-changed=../assets/logo/live_regie.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../assets/logo/live_regie.ico");
        res.set("ProductName", "Live Régie");
        res.set("FileDescription", "Live Régie — régie compagnon pour TikTok LIVE Studio");
        res.set("CompanyName", "Et si…");
        if let Err(e) = res.compile() {
            // Sans outil de ressources (rc.exe), l'exe est quand même produit, simplement sans icône
            println!("cargo:warning=Icône non intégrée : {}", e);
        }
    }
}
