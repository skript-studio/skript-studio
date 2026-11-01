/// Port of Java `ch.njol.skript.lang.ParseContext`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParseContext {
    #[default]
    Default,
    Config,
    Command,
    Parse,
}
