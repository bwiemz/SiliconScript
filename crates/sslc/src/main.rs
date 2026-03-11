use ssl_core::lexer::tokenize;
use ssl_core::parser::Parser;
use std::path::PathBuf;

fn read_source(args: &[String], cmd: &str) -> (PathBuf, String) {
    if args.len() < 3 {
        eprintln!("Usage: sslc {} <file.ssl>", cmd);
        std::process::exit(1);
    }
    let path = PathBuf::from(&args[2]);
    let source = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {}", path.display(), e);
        std::process::exit(1);
    });
    (path, source)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: sslc <command> [args]");
        eprintln!("Commands:  lex <file>  |  parse <file>");
        std::process::exit(1);
    }

    match args[1].as_str() {
        "lex" => {
            let (_path, source) = read_source(&args, "lex");
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
        "parse" => {
            let (_path, source) = read_source(&args, "parse");
            let tokens = tokenize(&source).unwrap_or_else(|e| {
                eprintln!("Lex error: {}", e);
                std::process::exit(1);
            });
            match Parser::parse(&source, tokens) {
                Ok(ast) => {
                    println!("{:#?}", ast);
                    eprintln!("\n{} top-level items", ast.items.len());
                }
                Err(e) => {
                    eprintln!("Parse error: {}", e);
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
