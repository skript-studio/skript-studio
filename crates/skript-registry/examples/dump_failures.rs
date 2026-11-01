fn main() {
    let mut reg = skript_registry::Registry::embedded().unwrap();
    let failures = reg.compile_all();
    println!("total failures: {}", failures.len());
    for f in &failures {
        let e = &reg.entries()[f.entry_index];
        println!(
            "[{}] {} / {} ({}):",
            e.id, e.addon.name, e.title, e.syntax_type
        );
        println!("    raw:    {}", f.raw.replace('\n', "\\n"));
        println!("    error:  {}", f.error.message);
    }
}
