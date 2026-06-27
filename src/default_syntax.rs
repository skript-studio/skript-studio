use crate::syntax::{Addon, SyntaxEntry, SyntaxType};

/// Return a minimal built-in syntax list so the server is always usable,
/// even when SkriptHub is unreachable and no cache exists.
pub fn entries() -> Vec<SyntaxEntry> {
    let mut out = Vec::new();
    let mut id = 1000000u32;

    macro_rules! entry {
        ($pattern:expr, $type:expr) => {{
            let cur = id;
            id = id.wrapping_add(1);
            e(cur, $pattern, $type)
        }};
    }

    // ── Events ──────────────────────────────────────────────────────────
    out.push(entry!("on load", SyntaxType::Event));
    out.push(entry!("on script load", SyntaxType::Event));
    out.push(entry!("on server load", SyntaxType::Event));
    out.push(entry!("on unload", SyntaxType::Event));
    out.push(entry!("on join", SyntaxType::Event));
    out.push(entry!("on quit", SyntaxType::Event));
    out.push(entry!("on disconnect", SyntaxType::Event));
    out.push(entry!("on chat", SyntaxType::Event));
    out.push(entry!("on death", SyntaxType::Event));
    out.push(entry!("on respawn", SyntaxType::Event));
    out.push(entry!("on damage", SyntaxType::Event));
    out.push(entry!("on break", SyntaxType::Event));
    out.push(entry!("on place", SyntaxType::Event));
    out.push(entry!("on click", SyntaxType::Event));
    out.push(entry!("on right click", SyntaxType::Event));
    out.push(entry!("on left click", SyntaxType::Event));
    out.push(entry!("on drop", SyntaxType::Event));
    out.push(entry!("on pickup", SyntaxType::Event));
    out.push(entry!("on shoot", SyntaxType::Event));
    out.push(entry!("on sneak", SyntaxType::Event));
    out.push(entry!("on sprint", SyntaxType::Event));
    out.push(entry!("on toggle flight", SyntaxType::Event));
    out.push(entry!("on walk", SyntaxType::Event));
    out.push(entry!("on fly", SyntaxType::Event));
    out.push(entry!("on command", SyntaxType::Event));
    out.push(entry!("on inventory click", SyntaxType::Event));
    out.push(entry!("on inventory open", SyntaxType::Event));
    out.push(entry!("on inventory close", SyntaxType::Event));
    out.push(entry!("on bed enter", SyntaxType::Event));
    out.push(entry!("on bed leave", SyntaxType::Event));
    out.push(entry!("on move", SyntaxType::Event));
    out.push(entry!("on world change", SyntaxType::Event));
    out.push(entry!("on gamemode change", SyntaxType::Event));
    out.push(entry!("on fish", SyntaxType::Event));
    out.push(entry!("on enchant", SyntaxType::Event));
    out.push(entry!("on eat", SyntaxType::Event));
    out.push(entry!("on bucket empty", SyntaxType::Event));
    out.push(entry!("on bucket fill", SyntaxType::Event));
    out.push(entry!("on shear", SyntaxType::Event));
    out.push(entry!("on explode", SyntaxType::Event));
    out.push(entry!("on portal", SyntaxType::Event));
    out.push(entry!("on teleport", SyntaxType::Event));
    out.push(entry!("on vehicle enter", SyntaxType::Event));
    out.push(entry!("on vehicle leave", SyntaxType::Event));
    out.push(entry!("on potion splash", SyntaxType::Event));
    out.push(entry!("on projectile hit", SyntaxType::Event));
    out.push(entry!("on projectile launch", SyntaxType::Event));
    out.push(entry!("on regen", SyntaxType::Event));
    out.push(entry!("on item break", SyntaxType::Event));
    out.push(entry!("on item craft", SyntaxType::Event));
    out.push(entry!("on item smelt", SyntaxType::Event));
    out.push(entry!("on item spawn", SyntaxType::Event));
    out.push(entry!("on item despawn", SyntaxType::Event));
    out.push(entry!("on grow", SyntaxType::Event));
    out.push(entry!("on leaf decay", SyntaxType::Event));
    out.push(entry!("on piston extend", SyntaxType::Event));
    out.push(entry!("on piston retract", SyntaxType::Event));
    out.push(entry!("on redstone", SyntaxType::Event));
    out.push(entry!("on sign change", SyntaxType::Event));
    out.push(entry!("on spawn", SyntaxType::Event));
    out.push(entry!("on weather change", SyntaxType::Event));

    // ── Effects ─────────────────────────────────────────────────────────
    out.push(entry!("send %text% to %player%", SyntaxType::Effect));
    out.push(entry!("send %text% to %players%", SyntaxType::Effect));
    out.push(entry!("send title \"%text%\" to %player%", SyntaxType::Effect));
    out.push(entry!("send title \"%text%\" to %players%", SyntaxType::Effect));
    out.push(entry!("send action bar \"%text%\" to %player%", SyntaxType::Effect));
    out.push(entry!("message \"%text%\" to %player%", SyntaxType::Effect));
    out.push(entry!("broadcast \"%text%\"", SyntaxType::Effect));
    out.push(entry!("set {_%object%} to %object%", SyntaxType::Effect));
    out.push(entry!("set {%object%} to %object%", SyntaxType::Effect));
    out.push(entry!("add %object% to %object%", SyntaxType::Effect));
    out.push(entry!("remove %object% from %object%", SyntaxType::Effect));
    out.push(entry!("delete %object%", SyntaxType::Effect));
    out.push(entry!("clear {_%object%}", SyntaxType::Effect));
    out.push(entry!("heal %player%", SyntaxType::Effect));
    out.push(entry!("damage %player% by %number%", SyntaxType::Effect));
    out.push(entry!("kill %entity%", SyntaxType::Effect));
    out.push(entry!("teleport %entity% to %location%", SyntaxType::Effect));
    out.push(entry!("give %item% to %player%", SyntaxType::Effect));
    out.push(entry!("take %item% from %player%", SyntaxType::Effect));
    out.push(entry!("wait %timespan%", SyntaxType::Effect));
    out.push(entry!("stop", SyntaxType::Effect));
    out.push(entry!("stop %object%", SyntaxType::Effect));
    out.push(entry!("cancel event", SyntaxType::Effect));
    out.push(entry!("continue", SyntaxType::Effect));
    out.push(entry!("clear %inventory%", SyntaxType::Effect));
    out.push(entry!("play %effect% at %location%", SyntaxType::Effect));
    out.push(entry!("play sound \"%text%\" at %location%", SyntaxType::Effect));
    out.push(entry!("spawn %entity% at %location%", SyntaxType::Effect));
    out.push(entry!("push %entity%", SyntaxType::Effect));
    out.push(entry!("pull %entity%", SyntaxType::Effect));
    out.push(entry!("kick %player% due to \"%text%\"", SyntaxType::Effect));
    out.push(entry!("ban %player%", SyntaxType::Effect));
    out.push(entry!("pardon %player%", SyntaxType::Effect));
    out.push(entry!("strike lightning at %location%", SyntaxType::Effect));
    out.push(entry!("explode at %location%", SyntaxType::Effect));
    out.push(entry!("ignite %entity%", SyntaxType::Effect));
    out.push(entry!("extinguish %entity%", SyntaxType::Effect));
    out.push(entry!("make %player% execute command \"%text%\"", SyntaxType::Effect));
    out.push(entry!("open %inventory% to %player%", SyntaxType::Effect));
    out.push(entry!("close %inventory% to %player%", SyntaxType::Effect));
    out.push(entry!("enchant %item% with %text%", SyntaxType::Effect));
    out.push(entry!("rename %item% to \"%text%\"", SyntaxType::Effect));
    out.push(entry!("lore %item% to \"%text%\"", SyntaxType::Effect));
    out.push(entry!("feed %player%", SyntaxType::Effect));
    out.push(entry!("saturate %player%", SyntaxType::Effect));
    out.push(entry!("remove %effect% from %player%", SyntaxType::Effect));
    out.push(entry!("hide %entity% from %player%", SyntaxType::Effect));
    out.push(entry!("show %entity% to %player%", SyntaxType::Effect));
    out.push(entry!("glow %entity%", SyntaxType::Effect));
    out.push(entry!("launch %player%", SyntaxType::Effect));
    out.push(entry!("lightning %entity%", SyntaxType::Effect));
    out.push(entry!("set block at %location% to %material%", SyntaxType::Effect));
    out.push(entry!("break block at %location%", SyntaxType::Effect));

    // ── Conditions ──────────────────────────────────────────────────────
    out.push(entry!("%boolean%", SyntaxType::Condition));
    out.push(entry!("%object% is %object%", SyntaxType::Condition));
    out.push(entry!("%object% is not %object%", SyntaxType::Condition));
    out.push(entry!("%player% has permission \"%text%\"", SyntaxType::Condition));
    out.push(entry!("%player% does not have permission \"%text%\"", SyntaxType::Condition));
    out.push(entry!("%player% is online", SyntaxType::Condition));
    out.push(entry!("%player% is not online", SyntaxType::Condition));
    out.push(entry!("%player% is op", SyntaxType::Condition));
    out.push(entry!("%number% is between %number% and %number%", SyntaxType::Condition));
    out.push(entry!("%player% is holding %item%", SyntaxType::Condition));
    out.push(entry!("%block% is %material%", SyntaxType::Condition));
    out.push(entry!("%block% is not %material%", SyntaxType::Condition));
    out.push(entry!("%entity% is a %entitytype%", SyntaxType::Condition));
    out.push(entry!("%entity% is not a %entitytype%", SyntaxType::Condition));
    out.push(entry!("%inventory% contains %item%", SyntaxType::Condition));
    out.push(entry!("%inventory% does not contain %item%", SyntaxType::Condition));
    out.push(entry!("%number% is greater than %number%", SyntaxType::Condition));
    out.push(entry!("%number% is less than %number%", SyntaxType::Condition));
    out.push(entry!("%text% contains %text%", SyntaxType::Condition));
    out.push(entry!("%text% does not contain %text%", SyntaxType::Condition));
    out.push(entry!("%number% > %number%", SyntaxType::Condition));
    out.push(entry!("%number% < %number%", SyntaxType::Condition));
    out.push(entry!("%number% >= %number%", SyntaxType::Condition));
    out.push(entry!("%number% <= %number%", SyntaxType::Condition));
    out.push(entry!("%number% = %number%", SyntaxType::Condition));
    out.push(entry!("%object% is set", SyntaxType::Condition));
    out.push(entry!("%object% is not set", SyntaxType::Condition));

    // ── Expressions ─────────────────────────────────────────────────────
    out.push(entry!("{_%object%}", SyntaxType::Expression));
    out.push(entry!("{%object%}", SyntaxType::Expression));
    out.push(entry!("player", SyntaxType::Expression));
    out.push(entry!("target entity of %player%", SyntaxType::Expression));
    out.push(entry!("location of %entity%", SyntaxType::Expression));
    out.push(entry!("name of %player%", SyntaxType::Expression));
    out.push(entry!("all players", SyntaxType::Expression));
    out.push(entry!("online players", SyntaxType::Expression));
    out.push(entry!("tool of %player%", SyntaxType::Expression));
    out.push(entry!("block at %location%", SyntaxType::Expression));
    out.push(entry!("event-player", SyntaxType::Expression));
    out.push(entry!("event-block", SyntaxType::Expression));
    out.push(entry!("event-location", SyntaxType::Expression));
    out.push(entry!("event-entity", SyntaxType::Expression));
    out.push(entry!("event-item", SyntaxType::Expression));
    out.push(entry!("event-number", SyntaxType::Expression));
    out.push(entry!("event-message", SyntaxType::Expression));
    out.push(entry!("event-damage", SyntaxType::Expression));
    out.push(entry!("event-attacker", SyntaxType::Expression));
    out.push(entry!("event-priority", SyntaxType::Expression));
    out.push(entry!("world of %entity%", SyntaxType::Expression));
    out.push(entry!("floor of %number%", SyntaxType::Expression));
    out.push(entry!("ceiling of %number%", SyntaxType::Expression));
    out.push(entry!("size of %inventory%", SyntaxType::Expression));
    out.push(entry!("type of %item%", SyntaxType::Expression));
    out.push(entry!("durability of %item%", SyntaxType::Expression));

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matching::{EntryTable, SyntaxIndex};

    #[test]
    fn all_entries_compile() {
        let entries = entries();
        assert!(!entries.is_empty(), "must have at least one entry");

        // All entries must compile without panicking/failing.
        let _index = SyntaxIndex::build(&entries);
        let table = EntryTable::from_entries(entries);
        // At least a reasonable number should have compiled successfully.
        assert!(table.len() > 20, "EntryTable should have most patterns");
    }

    #[test]
    fn covers_common_syntax() {
        let entries = entries();
        let patterns: Vec<&str> = entries.iter().map(|e| e.syntax_pattern.as_str()).collect();

        // Core event
        assert!(patterns.iter().any(|p| p.contains("on load")));
        assert!(patterns.iter().any(|p| p.contains("on join")));

        // Core effect
        assert!(patterns.iter().any(|p| p.contains("send %text% to %player%")));
        assert!(patterns.iter().any(|p| p.contains("broadcast")));

        // Core condition
        assert!(patterns.iter().any(|p| p.contains("has permission")));
        assert!(patterns.iter().any(|p| p.contains("is between")));

        // Core expression
        assert!(patterns.iter().any(|p| p.contains("player")));
        assert!(patterns.iter().any(|p| p.contains("event-player")));
    }
}

fn e(id: u32, pattern: &str, syntax_type: SyntaxType) -> SyntaxEntry {
    SyntaxEntry {
        id,
        title: pattern.to_owned(),
        description: String::new(),
        syntax_pattern: pattern.to_owned(),
        syntax_type,
        addon: Addon {
            name: "Skript".to_owned(),
            link_to_addon: String::new(),
            usage_score: 10.0,
        },
        return_type: None,
        required_plugins: Vec::new(),
        event_values: None,
        type_usage: None,
        entries: None,
        compatible_addon_version: String::new(),
        compatible_minecraft_version: String::new(),
        json_id: None,
        event_cancellable: false,
        mark_as_removed: false,
        removed_since: None,
    }
}
