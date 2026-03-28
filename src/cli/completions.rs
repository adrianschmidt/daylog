use clap::CommandFactory;
use clap_complete::Shell;

use super::Cli;

pub fn generate(shell: Shell) {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "daylog", &mut std::io::stdout());
}
