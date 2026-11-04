pub mod fs;
pub mod misc;
pub mod port;
pub mod re;
pub mod shell;
pub mod team;
pub mod venv;

use crate::tool::Registry;
use std::sync::Arc;

/// All built-in tools. `skills` feeds the `skill` tool's description.
pub fn builtin(skills: Vec<crate::skills::Skill>) -> Registry {
    let mut r = Registry::default();
    for t in [
        Arc::new(fs::Read) as Arc<dyn crate::tool::Tool>,
        Arc::new(fs::Ls),
        Arc::new(fs::GlobTool),
        Arc::new(fs::Grep),
        Arc::new(fs::Write),
        Arc::new(fs::Edit),
        Arc::new(shell::Bash),
        Arc::new(venv::Venv),
        Arc::new(team::Team),
        Arc::new(misc::TodoWrite),
        Arc::new(misc::WebFetch),
        Arc::new(misc::SkillTool(skills)),
        Arc::new(misc::Task),
        Arc::new(misc::Blocked),
        Arc::new(port::Survey),
        Arc::new(port::PinTool),
        Arc::new(port::EvidenceAdd),
        Arc::new(port::EvidenceList),
        Arc::new(port::Parity),
        Arc::new(port::Screenshot),
        Arc::new(port::SiteBuild),
        Arc::new(port::Scaffold),
        Arc::new(re::ReStatus),
        Arc::new(re::ReInstall),
        Arc::new(re::Disasm),
        Arc::new(re::Emulate),
        Arc::new(re::Decompile),
        Arc::new(re::Functions),
        Arc::new(re::Unity),
        Arc::new(re::Dotnet),
        Arc::new(re::UnityAssets),
        Arc::new(re::Hexdump),
    ] {
        r.add(t);
    }
    r
}
