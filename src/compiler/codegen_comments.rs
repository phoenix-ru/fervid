use super::{codegen::CodegenContext, imports::VueImports, helper::CodeHelper};

impl <'a> CodegenContext<'a> {
  pub fn create_comment_vnode(&mut self, buf: &mut String, comment_contents: &str) {
    buf.push_str(self.get_and_add_import_str(VueImports::CreateCommentVNode));
    CodeHelper::open_paren(buf);
    CodeHelper::quoted(buf, comment_contents);
    CodeHelper::close_paren(buf);
  }
}
