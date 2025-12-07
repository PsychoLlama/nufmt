use nu_cmd_lang::create_default_context;
use nu_parser::{flatten_block, parse, FlatShape};
use nu_protocol::engine::StateWorkingSet;

fn main() {
    let engine_state = create_default_context();
    let mut working_set = StateWorkingSet::new(&engine_state);
    
    let source = "# comment\nls | sort-by name # inline comment";
    let block = parse(&mut working_set, None, source.as_bytes(), false);
    let flattened = flatten_block(&working_set, &block);
    
    println!("Source: {:?}", source);
    println!("\nFlattened tokens:");
    for (span, shape) in &flattened {
        let token = &source[span.start..span.end];
        println!("  {:?}: {:?}", shape, token);
    }
}
