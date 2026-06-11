use super::*;
use crate::js_v8::selectors::query;

pub(crate) fn find_by_id(node: &NodePtr, id: &str) -> Option<NodePtr> {
    query::find_by_id(node, id)
}

pub(crate) fn collect_by_tag(node: &NodePtr, tag: &str, out: &mut Vec<NodePtr>) {
    query::collect_by_tag(node, tag, out)
}

pub(crate) fn collect_by_class(node: &NodePtr, cls: &str, out: &mut Vec<NodePtr>) {
    query::collect_by_class(node, cls, out)
}
