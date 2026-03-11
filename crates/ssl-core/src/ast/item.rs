use crate::span::{Span, Spanned};
use super::{Ident, Attribute, DocComment};
use super::expr::{Expr, CallArg};
use super::types::{TypeExpr, Direction, GenericParam};
use super::stmt::Stmt;

pub type Item = Spanned<ItemKind>;

#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    Module(ModuleDef),
    Struct(StructDef),
    Enum(EnumDef),
    Interface(InterfaceDef),
    FnDef(FnDef),
    Fsm(FsmDef),
    Pipeline(PipelineDef),
    Test(TestBlock),
    Import(ImportStmt),
    ExternModule(ExternModuleDef),
    Inst(InstDecl),
    GenFor(GenFor),
    GenIf(GenIf),
    Stmt(Stmt),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModuleDef {
    pub doc: Option<DocComment>,
    pub attrs: Vec<Attribute>,
    pub public: bool,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub ports: Vec<Port>,
    pub default_domain: Option<Ident>,
    pub body: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Port {
    pub doc: Option<DocComment>,
    pub direction: Direction,
    pub name: Ident,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDef {
    pub doc: Option<DocComment>,
    pub name: Ident,
    pub fields: Vec<StructField>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub name: Ident,
    pub ty: TypeExpr,
    pub bit_range: Option<(Expr, Expr)>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    pub doc: Option<DocComment>,
    pub name: Ident,
    pub encoding: Option<EnumEncoding>,
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnumEncoding { Binary, Onehot, Gray, Custom }

#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: Ident,
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceDef {
    pub doc: Option<DocComment>,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub groups: Vec<InterfaceGroup>,
    pub signals: Vec<InterfaceSignal>,
    pub properties: Vec<InterfaceProperty>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceGroup {
    pub name: Ident,
    pub signals: Vec<InterfaceSignal>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceSignal {
    pub name: Ident,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceProperty {
    pub name: Ident,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDef {
    pub doc: Option<DocComment>,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub params: Vec<FnParam>,
    pub return_type: TypeExpr,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnParam {
    pub name: Ident,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FsmDef {
    pub name: Ident,
    pub clock: Expr,
    pub reset: Expr,
    pub states: Vec<Ident>,
    pub encoding: Option<EnumEncoding>,
    pub initial: Ident,
    pub transitions: Vec<FsmTransition>,
    pub on_tick: Option<Vec<Stmt>>,
    pub outputs: Vec<FsmOutput>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FsmTransition {
    pub from: FsmStateRef,
    pub condition: FsmCondition,
    pub to: FsmStateRef,
    pub actions: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FsmStateRef { Named(Ident), Wildcard(Span) }

#[derive(Debug, Clone, PartialEq)]
pub enum FsmCondition {
    Expr(Expr),
    Timeout(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FsmOutput {
    pub state: Ident,
    pub assignments: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PipelineDef {
    pub name: Ident,
    pub clock: Expr,
    pub reset: Expr,
    pub backpressure: BackpressureMode,
    pub input: PipelinePort,
    pub output: PipelinePort,
    pub stages: Vec<PipelineStage>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BackpressureMode {
    Auto(Vec<CallArg>),
    Manual,
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PipelinePort {
    pub bindings: Vec<Ident>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PipelineStage {
    pub index: Expr,
    pub label: Option<String>,
    pub stall_when: Option<Expr>,
    pub flush_when: Option<Expr>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestBlock {
    pub name: String,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportStmt {
    pub names: Vec<Ident>,
    pub path: String,
    pub alias: Option<Ident>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternModuleDef {
    pub name: Ident,
    pub ports: Vec<Port>,
    pub backend: String,
    pub backend_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InstDecl {
    pub name: Ident,
    pub module_name: Ident,
    pub generic_args: Vec<Expr>,
    pub connections: Vec<PortConnection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortConnection {
    pub port: Ident,
    pub binding: PortBinding,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PortBinding {
    Input(Expr),
    Output(Expr),
    Bidirectional(Expr),
    Discard,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenFor {
    pub var: Ident,
    pub iterable: Expr,
    pub body: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenIf {
    pub condition: Expr,
    pub then_body: Vec<Item>,
    pub else_body: Option<Vec<Item>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceFile {
    pub items: Vec<Item>,
}
