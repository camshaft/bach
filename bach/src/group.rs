use crate::tracing::info_span;
use core::{
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use pin_project_lite::pin_project;
use std::{cell::RefCell, collections::HashMap};

thread_local! {
    static GROUPS: RefCell<Groups> = RefCell::new(Groups::default());
}

fn with<R>(f: impl FnOnce(&mut Groups) -> R) -> R {
    GROUPS.with(|groups| f(&mut groups.borrow_mut()))
}

#[derive(Default)]
struct Groups {
    name_to_id: HashMap<String, u64>,
    id_to_name: Vec<String>,
    id_to_tick_watermark: Vec<u64>,
}

impl Groups {
    fn name_to_id(&mut self, name: &str) -> u64 {
        if let Some(id) = self.name_to_id.get(name).copied() {
            return id;
        }

        let id = self.name_to_id.len() as u64;

        listener::try_borrow_with(|scope| {
            if let Some(on_group) = scope {
                on_group(id, name);
            }
        });

        self.name_to_id.insert(name.to_owned(), id);
        self.id_to_name.push(name.to_owned());
        self.id_to_tick_watermark.push(0);

        id
    }
}

pub(crate) fn list() -> Vec<Group> {
    with(|groups| {
        groups
            .id_to_name
            .iter()
            .enumerate()
            .map(|(id, _)| Group { id: id as _ })
            .collect()
    })
}

crate::scope::define!(scope, Group);
crate::scope::define!(listener, fn(u64, &str));

pub fn current() -> Group {
    scope::try_borrow_with(|scope| scope.unwrap_or_else(|| Group::new("main")))
}

pub(crate) fn reset() {
    with(|groups| {
        groups.id_to_tick_watermark.fill(0);
    });
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Group {
    id: u64,
}

impl fmt::Debug for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_struct("Group");
        f.field("id", &self.id);

        with(|groups| {
            if let Some(name) = groups.id_to_name.get(self.id as usize) {
                f.field("name", name);
            }
        });

        f.finish()
    }
}

impl fmt::Display for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        with(|groups| groups.id_to_name.get(self.id as usize).unwrap().fmt(f))
    }
}

impl Group {
    pub fn new(name: &str) -> Self {
        with(|groups| {
            let id = groups.name_to_id(name);

            Self { id }
        })
    }

    pub fn current() -> Self {
        current()
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn name(&self) -> String {
        self.to_string()
    }

    pub(crate) fn tick_watermark(&self) -> u64 {
        with(|groups| {
            groups
                .id_to_tick_watermark
                .get(self.id as usize)
                .copied()
                .unwrap_or(0)
        })
    }

    pub(crate) fn with_tick_watermark(&self, f: impl FnOnce(&mut u64)) {
        with(|groups| {
            let id = self.id as usize;
            let ticks = &mut groups.id_to_tick_watermark[id];
            f(ticks);
        })
    }
}

pub trait GroupExt: Sized {
    fn group<N: AsRef<str>>(self, name: N) -> Grouped<Self>;
}

impl<T> GroupExt for T
where
    T: Future,
{
    fn group<N: AsRef<str>>(self, name: N) -> Grouped<Self> {
        let group = Group::new(name.as_ref());
        Grouped::new(self, group)
    }
}

pin_project! {
    #[must_use = "futures do nothing unless polled"]
    pub struct Grouped<Inner> {
        #[pin]
        inner: Inner,
        group: Group,
        first: bool,
    }
}

impl<Inner> Grouped<Inner> {
    pub fn new(inner: Inner, group: Group) -> Self {
        Self {
            inner,
            group,
            first: true,
        }
    }
}

impl<Inner> Future for Grouped<Inner>
where
    Inner: Future,
{
    type Output = Inner::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        // if this is the first time polling the future then set the group on the task info
        if core::mem::take(this.first) {
            crate::task::info::scope::try_borrow_mut_with(|info| {
                if let Some(info) = info {
                    info.group = *this.group;
                }
            });
        }

        let inner = this.inner;
        let group = this.group;
        let span = info_span!("group", %group);
        let (_, res) = scope::with(*group, || span.in_scope(|| Future::poll(inner, cx)));
        res
    }
}
