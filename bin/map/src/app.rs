use crate::list::StatefulList;

pub struct App<'a> {
    pub items: StatefulList<(&'a str, usize)>,
}

impl<'a> App<'a> {
    pub fn new() -> App<'a> {
        App {
            items: StatefulList::with_items(vec![
                ("node0", 1),
                ("node1", 2),
                ("node2", 1),
                ("node3", 3),
                ("node4", 1),
                ("node5", 4),
                ("node6", 1),
                ("node7", 3),
                ("node8", 1),
                ("node9", 6),
                ("node10", 1),
                ("node11", 3),
                ("node12", 1),
                ("node13", 2),
                ("node14", 1),
                ("node15", 1),
                ("node16", 4),
                ("node17", 1),
                ("node18", 5),
                ("node19", 4),
                ("node20", 1),
                ("node21", 2),
                ("node22", 1),
                ("node23", 3),
                ("node24", 1),
            ]),
        }
    }
}
