use swc_css_ast::{PseudoClassSelectorChildren, PseudoElementSelectorChildren, Stylesheet};
use swc_css_codegen::{
    writer::basic::{BasicCssWriter, BasicCssWriterConfig},
    CodeGenerator, CodegenConfig, Emit,
};

pub struct StringifyOptions {
    pub minify: bool,
    pub basic_css_writer: BasicCssWriterConfig,
}

impl Default for StringifyOptions {
    fn default() -> Self {
        Self {
            minify: true,
            basic_css_writer: Default::default(),
        }
    }
}

/// Stringifies the [`Stylesheet`]
pub fn stringify(node: &Stylesheet, options: StringifyOptions) -> String {
    let mut buf = String::new();
    let writer = BasicCssWriter::new(&mut buf, None, options.basic_css_writer);
    let mut codegen = CodeGenerator::new(
        writer,
        CodegenConfig {
            minify: options.minify,
        },
    );
    let _ = codegen.emit(&node);

    buf
}

pub fn stringify_pseudo_class_selector_children(nodes: Vec<PseudoClassSelectorChildren>) -> String {
    let mut result = String::new();
    let writer = BasicCssWriter::new(&mut result, None, BasicCssWriterConfig::default());
    let mut codegen = CodeGenerator::new(writer, CodegenConfig { minify: true });

    // Taken from SWC `emit_list_pseudo_class_selector_children`
    // let len = nodes.len();

    for node in nodes.iter() {
        let _ = codegen.emit(node);

        // This is irrelevant, because `:deep` always contains `PreservedToken`s
        // if idx != len - 1 {
        //     match node {
        //         PseudoClassSelectorChildren::PreservedToken(_) => {}
        //         PseudoClassSelectorChildren::Delimiter(_) => {}
        //         _ => {
        //             let next = nodes.get(idx + 1);

        //             match next {
        //                 Some(PseudoClassSelectorChildren::Delimiter(Delimiter {
        //                     value: DelimiterValue::Comma,
        //                     ..
        //                 })) => {}
        //                 _ => {
        //                     space!(self)
        //                 }
        //             }
        //         }
        //     }
        // }
    }

    result
}

pub fn stringify_pseudo_element_selector_children(
    nodes: Vec<PseudoElementSelectorChildren>,
) -> String {
    let mut result = String::new();
    let writer = BasicCssWriter::new(&mut result, None, BasicCssWriterConfig::default());
    let mut codegen = CodeGenerator::new(writer, CodegenConfig { minify: true });

    // Taken from SWC `emit_list_pseudo_element_selector_children`
    // let len = nodes.len();

    for node in nodes.iter() {
        let _ = codegen.emit(node);

        // This is irrelevant, because `::v-deep` always contains `PreservedToken`s
        // if idx != len - 1 {
        //     match node {
        //         PseudoElementSelectorChildren::PreservedToken(_) => {}
        //         _ => {
        //             space!(self)
        //         }
        //     }
        // }
    }

    result
}
