use crate::ast::item::*;
use crate::ast::{Attribute, DocComment, Ident};
use crate::ast::types::Direction;
use crate::lexer::Token;
use crate::span::Spanned;
use super::expr::{parse_expr, parse_expr_in_generic};
use super::stmt::parse_stmt;
use super::types::{parse_generic_params, parse_type_expr, parse_type_expr_with_domain};
use super::{ParseError, Parser};

impl<'src> Parser<'src> {
    /// Parse a single top-level or nested item.
    pub fn parse_item(&mut self) -> Result<Item, ParseError> {
        let start = self.peek_span();

        // Collect leading doc comment (only the last contiguous one is kept)
        let mut doc: Option<DocComment> = None;
        if self.check(Token::DocComment) {
            let tok = self.advance();
            let mut text = self.text(tok.span).to_string();
            let first_span = tok.span;
            self.skip_newlines();
            while self.check(Token::DocComment) {
                let tok = self.advance();
                text.push('\n');
                text.push_str(self.text(tok.span));
                self.skip_newlines();
            }
            doc = Some(DocComment { text, span: first_span.merge(self.prev_span()) });
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
            Some(Token::KwStruct) => {
                if !attrs.is_empty() {
                    return Err(ParseError { message: "attributes are not supported on struct definitions".into(), span: attrs[0].span });
                }
                ItemKind::Struct(self.parse_struct_def(doc.take())?)
            }
            Some(Token::KwEnum) => {
                if !attrs.is_empty() {
                    return Err(ParseError { message: "attributes are not supported on enum definitions".into(), span: attrs[0].span });
                }
                ItemKind::Enum(self.parse_enum_def(doc.take())?)
            }
            Some(Token::KwInterface) => {
                if !attrs.is_empty() {
                    return Err(ParseError { message: "attributes are not supported on interface definitions".into(), span: attrs[0].span });
                }
                ItemKind::Interface(self.parse_interface_def(doc.take())?)
            }
            Some(Token::KwFn) => {
                if !attrs.is_empty() {
                    return Err(ParseError { message: "attributes are not supported on fn definitions".into(), span: attrs[0].span });
                }
                ItemKind::FnDef(self.parse_fn_def(doc.take())?)
            }
            Some(Token::KwFsm) => self.parse_fsm_def()?,
            Some(Token::KwPipeline) => self.parse_pipeline_def()?,
            Some(Token::KwTest) => ItemKind::Test(self.parse_test_block()?),
            Some(Token::KwImport) => ItemKind::Import(self.parse_import()?),
            Some(Token::KwExtern) => ItemKind::ExternModule(self.parse_extern_module()?),
            Some(Token::KwInst) => ItemKind::Inst(self.parse_inst_decl()?),
            Some(Token::KwGen) => self.parse_gen()?,
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
            self.parse_comma_list(Token::RParen, parse_expr)?
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

    /// Parse FSM definition with states, transitions, outputs.
    fn parse_fsm_def(&mut self) -> Result<ItemKind, ParseError> {
        self.expect_token(Token::KwFsm)?;
        let name = self.expect_ident()?;
        self.expect_token(Token::LParen)?;
        let clock = parse_expr(self)?;
        self.expect_token(Token::Comma)?;
        let reset = parse_expr(self)?;
        self.expect_token(Token::RParen)?;
        self.expect_token(Token::Colon)?;
        self.skip_newlines();
        self.expect_token(Token::Indent)?;

        let mut states = Vec::new();
        let mut encoding = None;
        let mut initial: Option<Ident> = None;
        let mut transitions = Vec::new();
        let mut on_tick: Option<Vec<crate::ast::stmt::Stmt>> = None;
        let mut outputs = Vec::new();

        while !self.check(Token::Dedent) && !self.is_at_end() {
            self.skip_newlines();
            if self.check(Token::Dedent) || self.is_at_end() { break; }

            // Handle `on tick:` — `on` is Token::KwOn keyword
            if self.check(Token::KwOn) {
                self.advance();
                if !self.check(Token::KwTick) {
                    return Err(self.error("expected 'tick' after 'on'"));
                }
                self.advance();
                on_tick = Some(self.parse_block(|p| parse_stmt(p))?);
                self.skip_newlines();
                continue;
            }

            let label = self.expect_ident()?;
            match label.node.as_str() {
                "states" => {
                    self.expect_token(Token::Colon)?;
                    states.push(self.expect_ident()?);
                    while self.eat(Token::Pipe).is_some() {
                        states.push(self.expect_ident()?);
                    }
                }
                "encoding" => {
                    self.expect_token(Token::Colon)?;
                    let enc_name = self.expect_ident()?;
                    encoding = Some(match enc_name.node.as_str() {
                        "binary" => EnumEncoding::Binary,
                        "onehot" => EnumEncoding::Onehot,
                        "gray" => EnumEncoding::Gray,
                        "custom" => EnumEncoding::Custom,
                        _ => return Err(ParseError {
                            message: format!("expected fsm encoding, found '{}'", enc_name.node),
                            span: enc_name.span,
                        }),
                    });
                }
                "initial" => {
                    self.expect_token(Token::Colon)?;
                    initial = Some(self.expect_ident()?);
                }
                "transitions" => {
                    let trans = self.parse_block(|p| p.parse_fsm_transition())?;
                    transitions = trans;
                }
                "outputs" => {
                    outputs = self.parse_block(|p| {
                        let out_start = p.peek_span();
                        let state = p.expect_ident()?;
                        p.expect_token(Token::FatArrow)?;
                        let assignments = vec![parse_stmt(p)?];
                        Ok(FsmOutput { state, assignments, span: out_start.merge(p.prev_span()) })
                    })?;
                }
                _ => return Err(ParseError {
                    message: format!("unexpected fsm section '{}'", label.node),
                    span: label.span,
                }),
            }
            self.skip_newlines();
        }
        self.expect_token(Token::Dedent)?;
        let init = initial.ok_or_else(|| self.error("fsm missing 'initial' state"))?;
        Ok(ItemKind::Fsm(FsmDef { name, clock, reset, states, encoding, initial: init, transitions, on_tick, outputs }))
    }

    fn parse_fsm_transition(&mut self) -> Result<FsmTransition, ParseError> {
        let start = self.peek_span();
        let from = if self.eat(Token::Underscore).is_some() {
            FsmStateRef::Wildcard(self.prev_span())
        } else { FsmStateRef::Named(self.expect_ident()?) };
        self.expect_token(Token::DashDash)?;
        let condition = if self.check(Token::KwIn) {
            // Not a timeout — fall through to expr
            self.expect_token(Token::LParen)?;
            let expr = parse_expr(self)?;
            self.expect_token(Token::RParen)?;
            FsmCondition::Expr(expr)
        } else if self.check_ident() && self.text(self.peek_span()) == "timeout" {
            self.advance();
            self.expect_token(Token::LParen)?;
            let expr = parse_expr(self)?;
            self.expect_token(Token::RParen)?;
            FsmCondition::Timeout(expr)
        } else {
            self.expect_token(Token::LParen)?;
            let expr = parse_expr(self)?;
            self.expect_token(Token::RParen)?;
            FsmCondition::Expr(expr)
        };
        self.expect_token(Token::LongArrow)?;
        let to = if self.eat(Token::Underscore).is_some() {
            FsmStateRef::Wildcard(self.prev_span())
        } else { FsmStateRef::Named(self.expect_ident()?) };
        let actions = if self.eat(Token::Colon).is_some() {
            if self.check(Token::Newline) || self.check(Token::Indent) {
                self.skip_newlines();
                if self.check(Token::Indent) {
                    self.expect_token(Token::Indent)?;
                    let mut stmts = Vec::new();
                    while !self.check(Token::Dedent) && !self.is_at_end() {
                        self.skip_newlines();
                        if self.check(Token::Dedent) { break; }
                        stmts.push(parse_stmt(self)?);
                        self.skip_newlines();
                    }
                    self.expect_token(Token::Dedent)?;
                    stmts
                } else { vec![parse_stmt(self)?] }
            } else { vec![parse_stmt(self)?] }
        } else { Vec::new() };
        Ok(FsmTransition { from, condition, to, actions, span: start.merge(self.prev_span()) })
    }

    fn parse_pipeline_def(&mut self) -> Result<ItemKind, ParseError> {
        self.expect_token(Token::KwPipeline)?;
        let name = self.expect_ident()?;
        self.expect_token(Token::LParen)?;
        let clock = parse_expr(self)?;
        self.expect_token(Token::Comma)?;
        let reset = parse_expr(self)?;
        let backpressure = if self.eat(Token::Comma).is_some() {
            let bp_name = self.expect_ident()?;
            if bp_name.node != "backpressure" {
                return Err(ParseError {
                    message: format!("expected 'backpressure', found '{}'", bp_name.node),
                    span: bp_name.span,
                });
            }
            self.expect_token(Token::Eq)?;
            let mode = self.expect_ident()?;
            match mode.node.as_str() {
                "auto" => BackpressureMode::Auto(Vec::new()),
                "manual" => BackpressureMode::Manual,
                "none" => BackpressureMode::None,
                _ => return Err(ParseError {
                    message: format!("expected backpressure mode, found '{}'", mode.node),
                    span: mode.span,
                }),
            }
        } else { BackpressureMode::Auto(Vec::new()) };
        self.expect_token(Token::RParen)?;
        self.expect_token(Token::Colon)?;
        self.skip_newlines();
        self.expect_token(Token::Indent)?;

        let input = self.parse_pipeline_port("input")?;
        self.skip_newlines();
        let output = self.parse_pipeline_port("output")?;
        self.skip_newlines();

        let mut stages = Vec::new();
        while !self.check(Token::Dedent) && !self.is_at_end() {
            self.skip_newlines();
            if self.check(Token::Dedent) || self.is_at_end() { break; }
            stages.push(self.parse_pipeline_stage()?);
            self.skip_newlines();
        }
        self.expect_token(Token::Dedent)?;
        Ok(ItemKind::Pipeline(PipelineDef { name, clock, reset, backpressure, input, output, stages }))
    }

    fn parse_pipeline_port(&mut self, expected_label: &str) -> Result<PipelinePort, ParseError> {
        let start = self.peek_span();
        let label = self.expect_ident()?;
        if label.node != expected_label {
            return Err(ParseError {
                message: format!("expected '{}', found '{}'", expected_label, label.node),
                span: label.span,
            });
        }
        self.expect_token(Token::Colon)?;
        let mut bindings = vec![self.expect_ident()?];
        while self.eat(Token::Comma).is_some() {
            bindings.push(self.expect_ident()?);
        }
        Ok(PipelinePort { bindings, span: start.merge(self.prev_span()) })
    }

    fn parse_pipeline_stage(&mut self) -> Result<PipelineStage, ParseError> {
        let start = self.peek_span();
        // `stage` is a keyword token Token::KwStage
        self.expect_token(Token::KwStage)?;
        let index = parse_expr(self)?;
        let label = if let Some(Token::StringLit(s)) = self.peek().cloned() {
            self.advance();
            Some(s)
        } else {
            None
        };
        self.expect_token(Token::Colon)?;
        self.skip_newlines();
        self.expect_token(Token::Indent)?;
        let (mut stall_when, mut flush_when, mut body) = (None, None, Vec::new());
        while !self.check(Token::Dedent) && !self.is_at_end() {
            self.skip_newlines();
            if self.check(Token::Dedent) || self.is_at_end() { break; }
            if self.check_ident() {
                let txt = self.text(self.peek_span()).to_string();
                if txt == "stall_when" || txt == "flush_when" {
                    self.advance();
                    self.expect_token(Token::Colon)?;
                    let expr = parse_expr(self)?;
                    if txt == "stall_when" { stall_when = Some(expr); } else { flush_when = Some(expr); }
                    self.skip_newlines();
                    continue;
                }
            }
            body.push(parse_stmt(self)?);
            self.skip_newlines();
        }
        self.expect_token(Token::Dedent)?;
        Ok(PipelineStage { index, label, stall_when, flush_when, body, span: start.merge(self.prev_span()) })
    }

    fn parse_test_block(&mut self) -> Result<TestBlock, ParseError> {
        self.expect_token(Token::KwTest)?;
        let name = match self.peek().cloned() {
            Some(Token::StringLit(s)) => { self.advance(); s }
            _ => return Err(self.error("expected test name string")),
        };
        let body = self.parse_block(|p| parse_stmt(p))?;
        Ok(TestBlock { name, body })
    }

    fn parse_import(&mut self) -> Result<ImportStmt, ParseError> {
        self.expect_token(Token::KwImport)?;
        let names = if self.eat(Token::LBrace).is_some() {
            self.parse_comma_list(Token::RBrace, |p| p.expect_ident())?
        } else {
            vec![self.expect_ident()?]
        };
        self.expect_token(Token::KwFrom)?;
        let path = match self.peek().cloned() {
            Some(Token::StringLit(s)) => { self.advance(); s }
            _ => return Err(self.error("expected import path string")),
        };
        let alias = if self.eat(Token::KwAs).is_some() { Some(self.expect_ident()?) } else { None };
        Ok(ImportStmt { names, path, alias })
    }

    fn parse_extern_module(&mut self) -> Result<ExternModuleDef, ParseError> {
        self.expect_token(Token::KwExtern)?;
        self.expect_token(Token::KwModule)?;
        let name = self.expect_ident()?;
        self.expect_token(Token::LParen)?;
        let ports = self.parse_comma_list(Token::RParen, |p| p.parse_port())?;
        self.expect_token(Token::At)?;
        let backend = self.expect_ident()?.node;
        self.expect_token(Token::LParen)?;
        let backend_name = match self.peek().cloned() {
            Some(Token::StringLit(s)) => { self.advance(); s }
            _ => return Err(self.error("expected backend module name string")),
        };
        self.expect_token(Token::RParen)?;
        Ok(ExternModuleDef { name, ports, backend, backend_name })
    }

    fn parse_inst_decl(&mut self) -> Result<InstDecl, ParseError> {
        self.expect_token(Token::KwInst)?;
        let name = self.expect_ident()?;
        self.expect_token(Token::Eq)?;
        let module_name = self.expect_ident()?;
        let generic_args = if self.eat(Token::Less).is_some() {
            self.parse_comma_list(Token::Greater, parse_expr_in_generic)?
        } else { Vec::new() };
        self.expect_token(Token::LParen)?;
        let connections = self.parse_comma_list(Token::RParen, |p| {
            let s = p.peek_span();
            let port = p.expect_ident()?;
            let binding = if p.eat(Token::Eq).is_some() {
                if p.eat(Token::Underscore).is_some() {
                    PortBinding::Discard
                } else {
                    PortBinding::Input(parse_expr(p)?)
                }
            } else if p.eat(Token::ThinArrow).is_some() {
                if p.eat(Token::Underscore).is_some() {
                    PortBinding::Discard
                } else {
                    PortBinding::Output(parse_expr(p)?)
                }
            } else if p.eat(Token::BiArrow).is_some() {
                PortBinding::Bidirectional(parse_expr(p)?)
            } else {
                return Err(p.error("expected '=', '->', or '<->' in port connection"));
            };
            Ok(PortConnection { port, binding, span: s.merge(p.prev_span()) })
        })?;
        Ok(InstDecl { name, module_name, generic_args, connections })
    }

    fn parse_gen(&mut self) -> Result<ItemKind, ParseError> {
        self.expect_token(Token::KwGen)?;
        match self.peek().cloned() {
            Some(Token::KwFor) => {
                self.advance();
                let var = self.expect_ident()?;
                self.expect_token(Token::KwIn)?;
                let iterable = parse_expr(self)?;
                let body = self.parse_block(|p| p.parse_item())?;
                Ok(ItemKind::GenFor(GenFor { var, iterable, body }))
            }
            Some(Token::KwIf) => {
                self.advance();
                let condition = parse_expr(self)?;
                let then_body = self.parse_block(|p| p.parse_item())?;
                let else_body = if self.check(Token::KwGen)
                    && self.tokens.get(self.pos + 1).map(|t| &t.node) == Some(&Token::KwElse)
                {
                    self.advance(); // consume gen
                    self.advance(); // consume else
                    Some(self.parse_block(|p| p.parse_item())?)
                } else {
                    None
                };
                Ok(ItemKind::GenIf(GenIf { condition, then_body, else_body }))
            }
            _ => Err(self.error("expected 'for' or 'if' after 'gen'")),
        }
    }

    /// Top-level: parse entire source file into a `SourceFile`.
    pub fn parse_file(&mut self) -> Result<SourceFile, ParseError> {
        let mut items = Vec::new();
        self.skip_newlines();
        while !self.is_at_end() {
            items.push(self.parse_item()?);
            self.skip_newlines();
        }
        Ok(SourceFile { items })
    }
}
