temos a implementação de um cliente pra api do roam (ou seja, sdk), porem esta interno no projeto
se outras pessoas que estão desenvolvendo em rust quiser integrar com roam, teram que desenvolver o que desenvolvemos (ou algo muito parecido)

vamos usar esse projeto pra ser o tui/cli e sdk, ou seja, ter um pipeline que transforma o cliente/api do roam em lib pra ser consumido em projetos rust

instando com `cargo install roam-sdk`

não devemos criar um novo projeto e sim estruturar uma pipeline que pegue o que criamos pra usar no roam-tui e faça a lib - acredito que isso se resolve de forma simples no proprio @cargo.toml em forma de distribuição, tem bin e lib
