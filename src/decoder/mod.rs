use std::fmt::Debug;

#[derive(Debug, Clone)]
pub enum Tree<T> {
    None,
    Leaf(T),
    Node {
        left: Box<Tree<T>>,
        right: Box<Tree<T>>,
    },
}

impl<T: Copy + Debug> Tree<T> {
    pub fn get(& self, size: u8, code: u16) -> Option<T> {
        if size > 16 {
            panic!("invalid code length");
        }

        match *self {
            Tree::None => {
                if size != 0 {
                    panic!("Invalid code used");
                }

                None
            },
            Tree::Leaf(v) => {
                if size != 0 {
                    panic!("Invalid code used");
                }

                Some(v)
            },
            Tree::Node {left: ref l, right: ref r} => {
                if size == 0 {
                    panic!("Invalid code used");
                }

                if code & 1 == 1 {
                    (&l).get(size - 1, code >> 1)
                } else {
                    (&r).get(size - 1, code >> 1)
                }
            }
        }
    }

    pub fn generate_tree(scv: &Vec<(u8, u16, Box<Tree<T>>)>)
            -> Box<Tree<T>> {

        if scv.len() == 1 {
            return scv[0].2.clone()
        }

        // Repeatedly merge adjacent nodes until one element remains: the tree itself.
        let mut sc = scv.clone();
        while sc.len() != 1 {
            sc = Tree::merge_adjacent(&mut sc);
        }

        return sc[0].2.clone()
    }

    fn merge_adjacent(scv: &mut Vec<(u8, u16, Box<Tree<T>>)>)
            -> Vec<(u8, u16, Box<Tree<T>>)> {

        let mut new_scv = Vec::with_capacity(scv.len());

        // Sort (size, code, value)-vector. Largest size first, then largest code first.
        scv.sort_by(|a, b| {
            let (size_a, code_a, _) = *a;
            let (size_b, code_b, _) = *b;
            let a = (size_a, code_a);
            let b = (size_b, code_b);
            b.cmp(&a)
        });

        let mut iter = scv.iter();
        let mut merged = 0;

        // Get next two elements
        let mut previous = iter.next().unwrap().clone();
        let mut tr;

        while let Some(current) = iter.next() {
            // If they are adjacent and belong to the same node, merge
            if (previous.0 == current.0) & ((previous.1 as i16 - current.1 as i16) == 1) {
                tr = Tree::Node {
                    left: previous.2,
                    right: current.2.clone()
                };

                // Increment merge counter, set merged as new previous
                merged += 1;
                previous.0 -= 1;
                previous.1 >>= 1;
                previous.2 = Box::new(tr);
            } else {
                // Not adjacent, push previous new_scv
                // Set current as new previous
                new_scv.push((previous.0, previous.1, previous.2));
                previous = current.clone();
            }
        }

        // Done, push previous to new_scv
        new_scv.push(previous);

        // If merged == 0, use first element and merge it with None.
        // Using first element is safe because sort order has not changed.
        if merged == 0 {
            tr = if new_scv[0].1 & 1 == 1 {
                Tree::Node {
                    left: new_scv[0].2.clone(),
                    right: Box::new(Tree::None)
                }
            } else {
                Tree::Node {
                    left: Box::new(Tree::None),
                    right: new_scv[0].2.clone()
                }
            };

            new_scv[0].0 -= 1;
            new_scv[0].1 >>= 1;
            new_scv[0].2 = Box::new(tr);
        }

        new_scv
    }
}