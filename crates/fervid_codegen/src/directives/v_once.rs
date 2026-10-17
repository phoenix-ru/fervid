use fervid_core::{CacheMarker, CacheMarkers};
use swc_core::ecma::ast::Expr;

use crate::CodegenContext;

impl CodegenContext {
    /// Generates a tracked vnode cache entry for `v-once`
    pub fn generate_v_once(&mut self, item_render_expr: Expr) -> Expr {
        let cache_idx = self.allocate_next_cache_entry();
        let markers = CacheMarkers::from(CacheMarker::NeedPauseTracking | CacheMarker::InVOnce);

        self.wrap_cache_expression(cache_idx, item_render_expr, markers)
    }
}

#[cfg(test)]
mod tests {
    use fervid_core::{CacheMarker, CacheMarkers};

    use crate::test_utils::js;

    use super::*;

    #[test]
    fn it_generates_v_once() {
        let mut ctx = CodegenContext::default();

        // Mock a render expression
        let item_render_expr = js("_createElementVNode(\"div\")");

        // First `v-once`
        let v_once_expr = ctx.generate_v_once(*item_render_expr.to_owned());
        assert_eq!(
            crate::test_utils::to_str(v_once_expr),
            "_cache[0]||(_setBlockTracking(-1,true),(_cache[0]=_createElementVNode(\"div\")).cacheIndex=0,_setBlockTracking(1),_cache[0])"
        );

        // Second `v-once` with increased cache index
        let v_once_expr = ctx.generate_v_once(*item_render_expr);
        assert_eq!(
            crate::test_utils::to_str(v_once_expr),
            "_cache[1]||(_setBlockTracking(-1,true),(_cache[1]=_createElementVNode(\"div\")).cacheIndex=1,_setBlockTracking(1),_cache[1])"
        );
    }

    #[test]
    fn it_generates_spread_array_cache() {
        let mut ctx = CodegenContext::default();
        let index = ctx.allocate_next_cache_entry();
        let markers = CacheMarkers::from(CacheMarker::NeedArraySpread);

        let cache = ctx.wrap_cache_expression(index, *js("items"), markers);

        assert_eq!(
            crate::test_utils::to_str(cache),
            "[...(_cache[0]||(_cache[0]=items))]"
        );
    }
}
