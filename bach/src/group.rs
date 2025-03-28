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

#[derive(Default)]
struct Groups {
    name_to_id: HashMap<String, u64>,
    id_to_name: HashMap<u64, String>,
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
        self.id_to_name.insert(id, name.to_owned());

        id
    }
}

pub(crate) fn list() -> Vec<Group> {
    GROUPS.with(|groups| {
        let groups = groups.borrow();
        groups
            .id_to_name
            .keys()
            .map(|id| Group { id: *id })
            .collect()
    })
}

crate::scope::define!(scope, Group);
crate::scope::define!(listener, fn(u64, &str));

pub fn current() -> Group {
    scope::try_borrow_with(|scope| scope.unwrap_or_else(|| Group::new("main")))
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Group {
    id: u64,
}

impl fmt::Debug for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_struct("Group");
        f.field("id", &self.id);

        GROUPS.with(|groups| {
            let groups = groups.borrow();
            if let Some(name) = groups.id_to_name.get(&self.id) {
                f.field("name", name);
            }
        });

        f.finish()
    }
}

impl fmt::Display for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        GROUPS.with(|groups| groups.borrow().id_to_name.get(&self.id).unwrap().fmt(f))
    }
}

impl Group {
    pub fn new(name: &str) -> Self {
        GROUPS.with(|groups| {
            let mut groups = groups.borrow_mut();
            let id = groups.name_to_id(name);

            Self { id }
        })
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn name(&self) -> String {
        self.to_string()
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
    }
}

impl<Inner> Grouped<Inner> {
    pub fn new(inner: Inner, group: Group) -> Self {
        Self { inner, group }
    }
}

impl<Inner> Future for Grouped<Inner>
where
    Inner: Future,
{
    type Output = Inner::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let inner = this.inner;
        let group = this.group;
        let span = info_span!("group", %group);
        let (_, res) = scope::with(*group, || span.in_scope(|| Future::poll(inner, cx)));
        res
    }
}
