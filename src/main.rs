use anyhow::Result;
use clap::Parser;

use webshelf::bootstrap::CliArgs;

#[tokio::main]
async fn main() -> Result<()> {
    let cli_args = CliArgs::parse();

    webshelf::bootstrap::init_logging(&cli_args.log_level);
    webshelf::bootstrap::setup_panic_handler();

    let bootstrap_result = webshelf::bootstrap::bootstrap(cli_args).await?;
    webshelf::bootstrap::start_server(bootstrap_result).await?;

    Ok(())
}
