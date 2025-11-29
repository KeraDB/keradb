use crate::storage::pager::Page;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Simple LRU cache for pages
pub struct BufferPool {
    cache: Arc<RwLock<HashMap<u32, Page>>>,
    max_size: usize,
}

impl BufferPool {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_size,
        }
    }

    pub fn get(&self, page_num: u32) -> Option<Page> {
        self.cache.read().get(&page_num).cloned()
    }

    pub fn put(&self, page: Page) {
        let mut cache = self.cache.write();
        
        // Simple eviction: remove random page if full
        if cache.len() >= self.max_size {
            if let Some(&key) = cache.keys().next() {
                cache.remove(&key);
            }
        }
        
        cache.insert(page.page_num, page);
    }

    pub fn remove(&self, page_num: u32) {
        self.cache.write().remove(&page_num);
    }

    pub fn clear(&self) {
        self.cache.write().clear();
    }

    pub fn size(&self) -> usize {
        self.cache.read().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PageType;

    #[test]
    fn test_buffer_pool() {
        let pool = BufferPool::new(2);
        
        let page1 = Page::new(0, PageType::Data, vec![1, 2, 3]);
        let page2 = Page::new(1, PageType::Data, vec![4, 5, 6]);
        
        pool.put(page1.clone());
        pool.put(page2.clone());
        
        assert_eq!(pool.size(), 2);
        assert!(pool.get(0).is_some());
        assert!(pool.get(1).is_some());
        
        // Adding another should evict one
        let page3 = Page::new(2, PageType::Data, vec![7, 8, 9]);
        pool.put(page3);
        
        assert_eq!(pool.size(), 2);
    }
}
