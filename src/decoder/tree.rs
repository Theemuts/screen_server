use std::fmt::Debug;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub enum Tree<T> {
    None,
    Leaf(T),
    Node {
        left: Rc<Tree<T>>,
        right: Rc<Tree<T>>,
    },
}

impl<T: Copy + Debug> Tree<T> {
    pub fn unwrap(&self) -> T {
        match *self {
            Tree::Leaf(v) => v,
            _ => { panic!("Supplied value is not a leaf node"); }
        }
    }

    pub fn get(&self, size: u8, code: u16) -> &Tree<T> {
        if size > 16 {
            panic!("invalid code length");
        }

        match self {
            &Tree::Node { left: ref l, right: ref r } => {
                if size == 0 { return self }

                let head = 0x8000 >> (16 - size) as u16; // first bit in code
                let trailing = if size > 1 { 0xFFFF >> (17 - size) as u16} else { 0u16 }; // trailing bits in code

                if (code & head) >> (size - 1) as u16 == 1 {
                    l.get(size - 1, code & trailing)
                } else {
                    r.get(size - 1, code & trailing)
                }
            },
            _ => {
                if size != 0 { panic!("Invalid code used") }
                self
            },
        }
    }

    // TODO: avoid excessive cloning
    pub fn generate_tree(size_code_value: &Vec<(u8, u16, Rc<Tree<T>>)>)
        -> Rc<Tree<T>> {
        let mut sc = size_code_value.clone();

        // Repeatedly merge adjacent nodes until one element remains: the tree itself.
        if size_code_value.len() > 1 {
            while sc.len() != 1 {
                sc = Tree::merge_adjacent(&mut sc);
            }
        }

        sc[0].2.clone()
    }

    fn merge_adjacent(size_code_value: &mut Vec<(u8, u16, Rc<Tree<T>>)>)
        -> Vec<(u8, u16, Rc<Tree<T>>)> {
        let mut new_size_code_value = Vec::with_capacity(size_code_value.len());

        // Sort (size, code, value)-vector. Largest size first, then largest code first.
        size_code_value.sort_by(|a, b| (b.0, b.1).cmp(&(a.0, a.1)));

        let mut iter = size_code_value.iter();
        let mut merged = 0;

        // Get next two elements
        let mut previous = iter.next().unwrap().clone();
        let mut node;

        while let Some(current) = iter.next() {
            // If they are adjacent and belong to the same node, merge
            if (previous.0 == current.0) & (previous.1 & 1 == 1) & ((previous.1 as i32 - current.1 as i32) == 1) {
                node = Rc::new(Tree::Node {
                    left: previous.2,
                    right: current.2.clone()
                });

                // Increment merge counter, set merged as new previous
                merged += 1;
                previous.0 -= 1; // decrement size
                previous.1 >>= 1; // right shift code
                previous.2 = node; // box the new node
            } else {
                // Not adjacent, push previous to new_size_code_value
                // Set current as new previous
                new_size_code_value.push((previous.0, previous.1, previous.2));
                previous = current.clone();
            }
        }

        // Done, push previous to new_size_code_value
        new_size_code_value.push(previous);

        // If merged == 0, use first element and merge it with None.
        // Using first element is safe because sort order has not changed.
        if merged == 0 {
            node = if new_size_code_value[0].1 & 1 == 1 {
                Rc::new(Tree::Node {
                    left: new_size_code_value[0].2.clone(),
                    right: Rc::new(Tree::None)
                })
            } else {
                Rc::new(Tree::Node {
                    left: Rc::new(Tree::None),
                    right: new_size_code_value[0].2.clone()
                })
            };

            new_size_code_value[0].0 -= 1;
            new_size_code_value[0].1 >>= 1;
            new_size_code_value[0].2 = node;
        }

        new_size_code_value
    }
}