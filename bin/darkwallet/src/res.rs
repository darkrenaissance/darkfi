use crate::error::{Error, Result};

pub type ResourceId = u32;

pub struct ResourceManager<T> {
    resources: Vec<(ResourceId, Option<T>)>,
    freed: Vec<usize>,
    id_counter: ResourceId,
}

impl<T> ResourceManager<T> {
    pub fn new() -> Self {
        Self { resources: vec![], freed: vec![], id_counter: 0 }
    }

    pub fn alloc(&mut self, rsrc: T) -> ResourceId {
        let id = self.id_counter;
        self.id_counter += 1;

        if self.freed.is_empty() {
            let idx = self.resources.len();
            self.resources.push((id, Some(rsrc)));
        } else {
            let idx = self.freed.pop().unwrap();
            let _ = std::mem::replace(&mut self.resources[idx], (id, Some(rsrc)));
        }
        id
    }

    pub fn get(&self, id: ResourceId) -> Option<&T> {
        for (idx, (rsrc_id, rsrc)) in self.resources.iter().enumerate() {
            if self.freed.contains(&idx) {
                continue
            }
            if *rsrc_id == id {
                return rsrc.as_ref()
            }
        }
        None
    }

    pub fn free(&mut self, id: ResourceId) -> Result<()> {
        for (idx, (rsrc_id, rsrc)) in self.resources.iter_mut().enumerate() {
            if self.freed.contains(&idx) {
                return Err(Error::ResourceNotFound)
            }
            if *rsrc_id == id {
                *rsrc = None;
                self.freed.push(idx);
                return Ok(())
            }
        }
        Err(Error::ResourceNotFound)
    }
}
