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
        loop {
            if let Some(v1) = iter.next() {
                if let Some(v2) = iter.next() {
                    // If they are adjacent and belong to the same node, merge
                    if (v1.0 == v2.0) & ((v1.1 as i16 - v2.1 as i16) == 1) {
                        let tr = Tree::Node {
                            left: v1.2.clone(),
                            right: v2.2.clone()
                        };

                        // Merging decreases size by 1, right shifts code by 1 (eg: 010 -> 01)
                        let size = v1.0 - 1;
                        let code = v1.1 >> 1;

                        // Increment merge counter, push merged to new_scv
                        merged += 1;
                        new_scv.push((size, code, Box::new(tr)))
                    } else {
                        // Not adjacent, push both new_scv
                        new_scv.push((v1.0, v1.1, v1.2.clone()));
                        new_scv.push((v2.0, v2.1, v2.2.clone()));
                    }
                } else {
                    // No v2, push v1 to new_scv
                    new_scv.push((v1.0, v1.1, v1.2.clone()));
                    break
                }
            } else {
                // End reached
                break
            }
        }

        // If merged == 0, use first element and merge it with None.
        if merged == 0 {
            let tr = if new_scv[0].1 & 1 == 1 {
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

            new_scv[0] = (new_scv[0].0 - 1, new_scv[0].1 >> 1, Box::new(tr));
        }

        new_scv
    }
}