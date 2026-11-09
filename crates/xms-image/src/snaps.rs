//! Snapshots: images kept, oldest first, to go back and forth between them and
//! the active one (the render coming in), as Houdini's render view. One list
//! for the program (`Snaps::shared`), so what one pane snaps another shows (the
//! Image Viewer and the Viewer); kept while the program runs. Each view says
//! which it shows by id, so another pane letting one go moves nothing.

use std::sync::{Arc, LazyLock, Mutex};

use crate::image::Image;

/// An image kept: what it was, and the name it is listed by.
pub struct Snap {
    pub id: usize,
    pub image: Arc<Image>,
    pub title: String,
    pub name: String,
}

/// The images kept, oldest first.
#[derive(Default)]
pub struct Snaps {
    pub list: Vec<Snap>,
    /// The last id given, the number in the last name.
    last: usize,
}

static SHARED: LazyLock<Arc<Mutex<Snaps>>> = LazyLock::new(Default::default);

impl Snaps {
    /// The program's: every view is given them unless given its own.
    pub fn shared() -> Arc<Mutex<Snaps>> {
        SHARED.clone()
    }

    /// Keeps an image, the newest. Its id.
    pub fn keep(&mut self, image: Arc<Image>, title: String) -> usize {
        self.last += 1;
        self.list.push(Snap {
            id: self.last,
            image,
            title,
            name: format!("Snap {}", self.last),
        });
        self.last
    }

    /// The one with this id, while it is kept.
    pub fn get(&self, id: Option<usize>) -> Option<&Snap> {
        self.list.iter().find(|s| Some(s.id) == id)
    }

    /// Where one is on the slider: the snapshots oldest first, then the active
    /// image, last (what one no longer kept shows).
    pub fn position(&self, id: Option<usize>) -> usize {
        id.and_then(|id| self.list.iter().position(|s| s.id == id))
            .unwrap_or(self.list.len())
    }

    /// Which is at a place on the slider: `None`, the active image.
    pub fn at(&self, position: usize) -> Option<usize> {
        self.list.get(position).map(|s| s.id)
    }

    /// Lets one go. What takes its place on the slider.
    pub fn remove(&mut self, id: usize) -> Option<usize> {
        let p = self.position(Some(id));
        if p < self.list.len() {
            self.list.remove(p);
        }
        self.at(p)
    }

    /// What they hold, in bytes (floats).
    pub fn bytes(&self) -> usize {
        self.list
            .iter()
            .map(|s| s.image.channels.len() * s.image.width * s.image.height * 4)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> Arc<Image> {
        Arc::new(Image::filled(2, 2, &["R", "G", "B"], 0.5))
    }

    #[test]
    fn kept_by_id_named_in_order() {
        let mut s = Snaps::default();
        let a = s.keep(image(), "render".into());
        let b = s.keep(image(), "viewer".into());
        assert_eq!((s.list[0].name.as_str(), s.list[1].name.as_str()), ("Snap 1", "Snap 2"));
        assert_eq!((s.position(Some(a)), s.position(Some(b)), s.position(None)), (0, 1, 2));
        assert_eq!((s.at(1), s.at(2)), (Some(b), None));
        assert_eq!(s.bytes(), 2 * 3 * 2 * 2 * 4);
    }

    /// Another pane letting one go: a view showing another keeps showing it, a
    /// view showing that one shows the active image (or the next).
    #[test]
    fn letting_one_go_moves_no_other_view() {
        let mut s = Snaps::default();
        let a = s.keep(image(), String::new());
        let b = s.keep(image(), String::new());
        assert_eq!(s.remove(a), Some(b), "the next one takes its place");
        assert_eq!(s.position(Some(b)), 0, "still shown where it now is");
        assert!(s.get(Some(a)).is_none());
        assert_eq!(s.position(Some(a)), 1, "gone: the active image");
        assert_eq!(s.remove(b), None);
        // Names go on counting: no two snapshots of a session share one.
        s.keep(image(), String::new());
        assert_eq!(s.list[0].name, "Snap 3");
    }
}
