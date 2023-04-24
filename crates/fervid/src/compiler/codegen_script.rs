use crate::parser::sfc_blocks::SfcScriptBlock;

use super::codegen::CodegenContext;

const EXPORT_DEFAULT: &str = "export default ";
const CONST_SFC: &str = "const __sfc__ = ";
const CONST_SFC_EMPTY: &str = "const __sfc__ = {}"; // to optimize insertion

impl<'a> CodegenContext<'a> {
    // Naive approach to use script: just replace `export default ` with `const __sfc__ = `
    // Todo use real parser in the future (e.g. swc_ecma_parser)
    // Todo support at max 2 script elements (do so in analyzer)
    pub fn compile_scripts(
        &mut self,
        buf: &mut String,
        legacy_script: Option<SfcScriptBlock>,
        setup_script: Option<SfcScriptBlock>,
    ) {
        // Only legacy script scenario is currently implemented
        if let (Some(script_content), None) = (legacy_script, setup_script) {
            // Naive approach: replace
            let has_default_export = script_content.content.contains(EXPORT_DEFAULT);

            if has_default_export {
                buf.push_str(&script_content.content.replace(EXPORT_DEFAULT, CONST_SFC));
                self.code_helper.newline(buf);
                return;
            }
        }

        // todo sensible default?
        buf.push_str(CONST_SFC_EMPTY);
        self.code_helper.newline(buf)
    }
}
