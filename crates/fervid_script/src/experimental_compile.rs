use std::sync::Arc;

use swc_core::{
    common::{SourceMap, DUMMY_SP},
    ecma::{
        ast::{
            CallExpr, Callee, Expr, ExprOrSpread, Ident, KeyValueProp, Lit, ObjectLit, Prop,
            PropName, PropOrSpread, Str,
        },
        atoms::JsWord,
    },
};
use swc_ecma_codegen::{text_writer::JsWriter, Emitter, Node as _};

lazy_static! {
    static ref OPEN_BLOCK: JsWord = JsWord::from("_openBlock");
    static ref CREATE_BLOCK: JsWord = JsWord::from("_createBlock");
}

pub fn experimental_compile() {
    let mut created_block = create_block();
    add_component(&mut created_block, JsWord::from("_component_abc_def"));

    // add some random attributes
    // in real scenario all the attributes will be processed
    // here we don't need to do that for demo purposes
    add_attributes(
        &mut created_block,
        vec![
            ("modelValue", "_ctx.modelValue"),
            (
                "onUpdate:modelValue",
                "$event => ((_ctx.modelValue) = $event)",
            ),
            ("modelModifiers", "{lazy: true}"),
            ("another-model-value", "_ctx.modelValue"),
            (
                "onUpdate:anotherModelValue",
                "$event => ((_ctx.modelValue) = $event)",
            ),
            ("another-model-valueModifiers", "{trim: true}"),
            ("test-bound", "_ctx.bar+_ctx.baz"),
            ("disabled", "disabled"),
            ("onClick", r#"_withModifiers(() => {}, ["prevent"])"#),
            ("onHello", "_ctx.world"),
            ("class", ""),
        ],
    );

    let compiled_element = Expr::Call(created_block);

    // Emitting the result requires some setup with SWC
    let cm: Arc<SourceMap> = Default::default();
    let mut buff: Vec<u8> = Vec::new();
    let writer: JsWriter<&mut Vec<u8>> = JsWriter::new(cm.clone(), "\n", &mut buff, None);

    let mut emitter = Emitter {
        cfg: swc_ecma_codegen::Config {
            target: Default::default(),
            ascii_only: false,
            minify: false,
            omit_last_semi: false,
        },
        comments: None,
        wr: writer,
        cm,
    };

    let _ = compiled_element.emit_with(&mut emitter);

    let result = String::from_utf8(buff).map_err(|_| ());

    println!("{}", result.unwrap());
}

// _createBlock()
fn create_block() -> CallExpr {
    let callee = Callee::Expr(Box::from(Expr::Ident(Ident::new(
        OPEN_BLOCK.to_owned(),
        DUMMY_SP,
    ))));

    CallExpr {
        span: DUMMY_SP,
        callee,
        args: vec![],
        type_args: None,
    }
}

fn add_component(create_block_expr: &mut CallExpr, component: JsWord) {
    create_block_expr.args.insert(
        0,
        ExprOrSpread {
            spread: None,
            expr: Box::from(Expr::Ident(Ident::new(component, DUMMY_SP))),
        },
    );
}

fn add_attributes(create_block_expr: &mut CallExpr, attrs: Vec<(&str, &str)>) {
    let mut attrs_obj = ObjectLit {
        span: DUMMY_SP,
        props: Vec::with_capacity(attrs.len()),
    };

    // just dump whatever is inside
    for attr in attrs {
        attrs_obj
            .props
            .push(PropOrSpread::Prop(Box::from(Prop::KeyValue(
                KeyValueProp {
                    key: PropName::Str(Str::from(JsWord::from(attr.0))),
                    value: Expr::Lit(Lit::Str(Str::from(JsWord::from(attr.1)))).into(),
                },
            ))));
    }

    create_block_expr.args.insert(
        1,
        ExprOrSpread {
            spread: None,
            expr: Box::from(Expr::Object(attrs_obj)),
        },
    );
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn experimental_template_compilation() {
        // warm-up run
        let inst = std::time::Instant::now();
        experimental_compile();
        println!("Elapsed {:?}", inst.elapsed());

        // warmed-up run
        let inst = std::time::Instant::now();
        experimental_compile();
        println!("Elapsed {:?}", inst.elapsed())
    }
}
