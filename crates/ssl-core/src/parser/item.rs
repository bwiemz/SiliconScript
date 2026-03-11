use crate::ast::item::*;
use crate::ast::{Attribute, DocComment};
use crate::ast::types::Direction;
use crate::lexer::Token;
use crate::span::Spanned;
use super::expr::parse_expr;
use super::stmt::parse_stmt;
use super::types::{parse_generic_params, parse_type_expr, parse_type_expr_with_domain};
use super::{ParseError, Parser};

impl<'src> Parser<'src> {
    /// Parse a single top-level or nested item.
    pub fn parse_item(&mut self) -> Result<Item, ParseError> {
        let start = self.peek_span();

        // Collect leading doc comments
        let mut doc: Option<DocComment> = None;
        while self.check(Token::DocComment) {
            let tok = self.advance();
            let text = self.text(tok.span).to_string();
            doc = Some(DocComment { text, span: tok.span });
            self.skip_newlines();
        }

        // Collect leading attributes
        let mut attrs = Vec::new();
        while self.check(Token::At) {
            attrs.push(self.parse_attribute()?);
            self.skip_newlines();
        }

        // Check for pub modifier
        let public = self.eat(Token::KwPub).is_some();

        let kind = match self.peek().cloned() {
            Some(Token::KwModule) => {
                ItemKind::Module(self.parse_module_def(doc.take(), attrs.drain(..).collect(), public)?)
            }
            Some(Token::KwStruct) => ItemKind::Struct(self.parse_struct_def(doc.take())?),
            Some(Token::KwEnum) => ItemKind::Enum(self.parse_enum_def(doc.take())?),
            Some(Token::KwInterface) => ItemKind::Interface(self.parse_interface_def(doc.take())?),
            Some(Token::KwFn) => ItemKind::FnDef(self.parse_fn_def(doc.take())?),
            Some(Token::KwFsm) => return Err(self.error("FSM parsing not yet implemented")),
            Some(Token::KwPipeline) => return Err(self.error("pipeline parsing not yet implemented")),
            Some(Token::KwTest) => return Err(self.error("test block parsing not yet implemented")),
            Some(Token::KwImport) => return Err(self.error("import parsing not yet implemented")),
            Some(Token::KwExtern) => return Err(self.error("extern module parsing not yet implemented")),
            Some(Token::KwInst) => return Err(self.error("inst parsing not yet implemented")),
            Some(Token::KwGen) => return Err(self.error("gen parsing not yet implemented")),
            _ => {
                // Fall back to statement
                let stmt = parse_stmt(self)?;
                ItemKind::Stmt(stmt)
            }
        };

        Ok(Spanned::new(kind, start.merge(self.prev_span())))
    }

    /// Parse `@ IDENT [( ARGS )]`
    fn parse_attribute(&mut self) -> Result<Attribute, ParseError> {
        let start = self.peek_span();
        self.expect_token(Token::At)?;
        let name = self.expect_ident()?;
        let args = if self.eat(Token::LParen).is_some() {
            self.parse_comma_list(Token::RParen, |p| {
                let e = parse_expr(p)?;
                let span = e.span;
                Ok(Spanned::new(e, span))
            })?
        } else {
            Vec::new()
        };
        let span = start.merge(self.prev_span());
        Ok(Attribute { name, args, span })
    }

    /// `[pub] module NAME [<GENERICS>] ( PORTS ) [@ DOMAIN]: INDENT ITEMS DEDENT`
    fn parse_module_def(
        &mut self,
        doc: Option<DocComment>,
        attrs: Vec<Attribute>,
        public: bool,
    ) -> Result<ModuleDef, ParseError> {
        self.expect_token(Token::KwModule)?;
        let name = self.expect_ident()?;
        let generics = parse_generic_params(self)?;
        self.expect_token(Token::LParen)?;
        let ports = self.parse_comma_list(Token::RParen, |p| p.parse_port())?;
        let default_domain = if self.eat(Token::At).is_some() {
            Some(self.expect_ident()?)
        } else {
            None
        };
        let body = self.parse_block(|p| p.parse_item())?;
        Ok(ModuleDef {
            doc,
            attrs,
            public,
            name,
            generics,
            ports,
            default_domain,
            body,
        })
    }

    /// Parse a single port: `DIR NAME: TYPE [@ DOMAIN]`
    pub(crate) fn parse_port(&mut self) -> Result<Port, ParseError> {
        let start = self.peek_span();
        let doc = if self.check(Token::DocComment) {
            let tok = self.advance();
            self.skip_newlines();
            Some(DocComment {
                text: self.text(tok.span).to_string(),
                span: tok.span,
            })
        } else {
            None
        };
        let direction = match self.peek().cloned() {
            Some(Token::KwIn) => {
                self.advance();
                Direction::In
            }
            Some(Token::KwOut) => {
                self.advance();
                Direction::Out
            }
            Some(Token::KwInout) => {
                self.advance();
                Direction::InOut
            }
            _ => return Err(self.error("expected port direction (in, out, inout)")),
        };
        let name = self.expect_ident()?;
        self.expect_token(Token::Colon)?;
        let ty = parse_type_expr_with_domain(self)?;
        Ok(Port {
            doc,
            direction,
            name,
            ty,
            span: start.merge(self.prev_span()),
        })
    }

    /// `struct NAME: INDENT (NAME: TYPE [@ [H:L]])* DEDENT`
    fn parse_struct_def(&mut self, doc: Option<DocComment>) -> Result<StructDef, ParseError> {
        self.expect_token(Token::KwStruct)?;
        let name = self.expect_ident()?;
        let fields = self.parse_block(|p| {
            let field_start = p.peek_span();
            let fname = p.expect_ident()?;
            p.expect_token(Token::Colon)?;
            let ty = parse_type_expr(p)?;
            let bit_range = if p.eat(Token::At).is_some() {
                p.expect_token(Token::LBracket)?;
                let hi = parse_expr(p)?;
                p.expect_token(Token::Colon)?;
                let lo = parse_expr(p)?;
                p.expect_token(Token::RBracket)?;
                Some((hi, lo))
            } else {
                None
            };
            Ok(StructField {
                name: fname,
                ty,
                bit_range,
                span: field_start.merge(p.prev_span()),
            })
        })?;
        Ok(StructDef { doc, name, fields })
    }

    /// `enum NAME [encoding]: INDENT (VARIANT [= EXPR])* DEDENT`
    fn parse_enum_def(&mut self, doc: Option<DocComment>) -> Result<EnumDef, ParseError> {
        self.expect_token(Token::KwEnum)?;
        let name = self.expect_ident()?;
        let encoding = if self.eat(Token::LBracket).is_some() {
            let enc_name = self.expect_ident()?;
            let enc = match enc_name.node.as_str() {
                "binary" => EnumEncoding::Binary,
                "onehot" => EnumEncoding::Onehot,
                "gray" => EnumEncoding::Gray,
                "custom" => EnumEncoding::Custom,
                _ => {
                    return Err(ParseError {
                        message: format!(
                            "expected encoding (binary/onehot/gray/custom), found '{}'",
                            enc_name.node
                        ),
                        span: enc_name.span,
                    })
                }
            };
            self.expect_token(Token::RBracket)?;
            Some(enc)
        } else {
            None
        };
        let variants = self.parse_block(|p| {
            let var_start = p.peek_span();
            let vname = p.expect_ident()?;
            let value = if p.eat(Token::Eq).is_some() {
                Some(parse_expr(p)?)
            } else {
                None
            };
            Ok(EnumVariant {
                name: vname,
                value,
                span: var_start.merge(p.prev_span()),
            })
        })?;
        Ok(EnumDef {
            doc,
            name,
            encoding,
            variants,
        })
    }

    /// `interface NAME [<GENERICS>]: INDENT (group|signal|property)* DEDENT`
    fn parse_interface_def(
        &mut self,
        doc: Option<DocComment>,
    ) -> Result<InterfaceDef, ParseError> {
        self.expect_token(Token::KwInterface)?;
        let name = self.expect_ident()?;
        let generics = parse_generic_params(self)?;
        self.expect_token(Token::Colon)?;
        self.skip_newlines();
        self.expect_token(Token::Indent)?;
        let (mut groups, mut signals, mut properties) = (Vec::new(), Vec::new(), Vec::new());
        while !self.check(Token::Dedent) && !self.is_at_end() {
            self.skip_newlines();
            if self.check(Token::Dedent) || self.is_at_end() {
                break;
            }
            let s = self.peek_span();
            // `group` is a keyword token — handle it explicitly
            if self.check(Token::KwGroup) {
                self.advance();
                let gn = self.expect_ident()?;
                let gs = self.parse_block(|p| {
                    let ss = p.peek_span();
                    let sn = p.expect_ident()?;
                    p.expect_token(Token::Colon)?;
                    let st = parse_type_expr(p)?;
                    Ok(InterfaceSignal {
                        name: sn,
                        ty: st,
                        span: ss.merge(p.prev_span()),
                    })
                })?;
                groups.push(InterfaceGroup {
                    name: gn,
                    signals: gs,
                    span: s.merge(self.prev_span()),
                });
                self.skip_newlines();
                continue;
            }
            let label = self.expect_ident()?;
            match label.node.as_str() {
                "property" => {
                    let pn = self.expect_ident()?;
                    self.expect_token(Token::Colon)?;
                    self.skip_newlines();
                    let body = if self.check(Token::Indent) {
                        self.expect_token(Token::Indent)?;
                        self.skip_newlines();
                        let e = parse_expr(self)?;
                        self.skip_newlines();
                        self.expect_token(Token::Dedent)?;
                        e
                    } else {
                        parse_expr(self)?
                    };
                    properties.push(InterfaceProperty {
                        name: pn,
                        body,
                        span: s.merge(self.prev_span()),
                    });
                }
                _ => {
                    // Treat as a signal: NAME: TYPE
                    self.expect_token(Token::Colon)?;
                    let ty = parse_type_expr(self)?;
                    signals.push(InterfaceSignal {
                        name: label,
                        ty,
                        span: s.merge(self.prev_span()),
                    });
                }
            }
            self.skip_newlines();
        }
        self.expect_token(Token::Dedent)?;
        Ok(InterfaceDef {
            doc,
            name,
            generics,
            groups,
            signals,
            properties,
        })
    }

    /// `fn NAME [<GENERICS>] ( PARAMS ) -> TYPE: INDENT STMTS DEDENT`
    fn parse_fn_def(&mut self, doc: Option<DocComment>) -> Result<FnDef, ParseError> {
        self.expect_token(Token::KwFn)?;
        let name = self.expect_ident()?;
        let generics = parse_generic_params(self)?;
        self.expect_token(Token::LParen)?;
        let params = self.parse_comma_list(Token::RParen, |p| {
            let param_start = p.peek_span();
            let pname = p.expect_ident()?;
            p.expect_token(Token::Colon)?;
            let pty = parse_type_expr(p)?;
            Ok(FnParam {
                name: pname,
                ty: pty,
                span: param_start.merge(p.prev_span()),
            })
        })?;
        self.expect_token(Token::ThinArrow)?;
        let return_type = parse_type_expr(self)?;
        let body = self.parse_block(|p| parse_stmt(p))?;
        Ok(FnDef {
            doc,
            name,
            generics,
            params,
            return_type,
            body,
        })
    }
}
