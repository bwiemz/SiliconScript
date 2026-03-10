use std::path::PathBuf;
use ssl_core::lexer::tokenize;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: sslc <command> [args]");
        eprintln!("Commands:");
        eprintln!("  lex <file.ssl>    Tokenize a file and print tokens");
        std::process::exit(1);
    }

    match args[1].as_str() {
        "lex" => {
            if args.len() < 3 {
                eprintln!("Usage: sslc lex <file.ssl>");
                std::process::exit(1);
            }
            let path = PathBuf::from(&args[2]);
            let source = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error reading {}: {}", path.display(), e);
                    std::process::exit(1);
                }
            };

            match tokenize(&source) {
                Ok(tokens) => {
                    for tok in &tokens {
                        println!("{:>4}..{:<4} {:?}", tok.span.start, tok.span.end, tok.node);
                    }
                    eprintln!("\n{} tokens", tokens.len());
                }
                Err(e) => {
                    eprintln!("Lex error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        other => {
            eprintln!("Unknown command: {}", other);
            std::process::exit(1);
        }
    }
}
