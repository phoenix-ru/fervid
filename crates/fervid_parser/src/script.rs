use fervid_core::{SfcScriptBlock, SfcScriptLang};
use swc_core::{ecma::ast::{Expr, Pat, Module}, common::{comments::SingleThreadedComments, Span}};
use swc_ecma_parser::{PResult, lexer::Lexer, Syntax, StringInput, Parser, TsConfig, EsConfig};
use swc_html_ast::{Element, Child};

pub fn parse_sfc_script_element(element: Element) -> Option<SfcScriptBlock> {
    let mut is_setup = false;
    let mut lang = SfcScriptLang::Es;

    for attr in element.attributes.iter() {
        let attr_name = &attr.name;
        if attr_name.eq("setup") {
            is_setup = true;
        } else if attr_name.eq("lang") {
            lang = match attr.value.as_ref() {
                Some(v) if v.eq("ts") => SfcScriptLang::Typescript,
                _ => SfcScriptLang::Es
            };
        }
    }

    // `<script>` should always have a single `Text` child
    let Some(Child::Text(script_content)) = element.children.get(0) else {
        return None;
    };

    let syntax = if matches!(lang, SfcScriptLang::Typescript) {
        Syntax::Typescript(TsConfig::default())
    } else {
        Syntax::Es(EsConfig::default())
    };

    let Ok(content) = parse_module(&script_content.data, syntax, script_content.span) else {
        // TODO Error??
        return None;
    };

    Some(SfcScriptBlock {
        content: Box::new(content),
        lang,
        is_setup,
    })
}

pub fn parse_expr(raw: &str, syntax: Syntax, span: Span) -> PResult<Box<Expr>> {
    let comments = SingleThreadedComments::default();

    let lexer = Lexer::new(
        syntax,
        // EsVersion defaults to es5
        Default::default(),
        StringInput::new(raw, span.lo, span.hi),
        Some(&comments)
    );

    let mut parser = Parser::new_from(lexer);

    // TODO Return comments or use parser instance to store it
    parser.parse_expr()
}

pub fn parse_module(raw: &str, syntax: Syntax, span: Span) -> PResult<Module> {
    let lexer = Lexer::new(
        syntax,
        // EsVersion defaults to es5
        Default::default(),
        StringInput::new(raw, span.lo, span.hi),
        None,
    );

    let mut parser = Parser::new_from(lexer);

    // TODO Return comments or use parser instance to store it
    parser.parse_module()
}

pub fn parse_pat(raw: &str, syntax: Syntax, span: Span) -> PResult<Pat> {
    let lexer = Lexer::new(
        syntax,
        // EsVersion defaults to es5
        Default::default(),
        StringInput::new(raw, span.lo, span.hi),
        None,
    );

    let mut parser = Parser::new_from(lexer);

    // TODO Return comments or use parser instance to store it
    parser.parse_pat()
}
