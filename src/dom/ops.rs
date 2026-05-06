use super::{Node, NodePtr};

impl Node {
    /// Find a node by its ID attribute and return the path to it.
    pub fn find_node_by_id(&self, id: &str) -> Option<Vec<usize>> {
        let mut path = Vec::new();
        if self.find_node_by_id_recursive(id, &mut path) {
            Some(path)
        } else {
            None
        }
    }

    fn find_node_by_id_recursive(&self, id: &str, path: &mut Vec<usize>) -> bool {
        match self {
            Node::Document { children } => {
                for (i, child) in children.iter().enumerate() {
                    path.push(i);
                    if child.borrow().find_node_by_id_recursive(id, path) {
                        return true;
                    }
                    path.pop();
                }
                false
            }
            Node::Element(element) => {
                if element
                    .attributes
                    .get("id")
                    .map(|v| v == id)
                    .unwrap_or(false)
                {
                    return true;
                }
                for (i, child) in element.children.iter().enumerate() {
                    path.push(i);
                    if child.borrow().find_node_by_id_recursive(id, path) {
                        return true;
                    }
                    path.pop();
                }
                false
            }
            Node::Text(_) => false,
        }
    }

    /// Get a child node by index from a document or element.
    pub fn get_child_mut(&mut self, index: usize) -> Option<NodePtr> {
        match self {
            Node::Document { children } => children.get(index).cloned(),
            Node::Element(element) => element.children.get(index).cloned(),
            Node::Text(_) => None,
        }
    }
}
