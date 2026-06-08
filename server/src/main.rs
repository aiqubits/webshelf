use anyhow::Result;
use clap::Parser;

use webshelf_server::bootstrap::CliArgs;

#[tokio::main]
async fn main() -> Result<()> {
    let cli_args = CliArgs::parse();

    webshelf_server::bootstrap::init_logging(&cli_args.log_level);
    webshelf_server::bootstrap::setup_panic_handler();

    let bootstrap_result = webshelf_server::bootstrap::bootstrap(cli_args).await?;
    webshelf_server::bootstrap::start_server(bootstrap_result).await?;

    Ok(())
}
