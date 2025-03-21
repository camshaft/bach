pub use std::cell::RefCell;

#[doc(hidden)]
#[macro_export]
macro_rules! define {
    ($name:ident, $ty:ty) => {
        pub mod $name {
            #[allow(unused_imports)]
            use super::*;
            use $crate::scope::RefCell;

            thread_local! {
                static SCOPE: RefCell<Option<$ty>> = RefCell::new(None);
            }

            #[allow(dead_code)]
            pub fn set(value: Option<$ty>) -> Option<$ty> {
                try_borrow_mut_with(|r| core::mem::replace(r, value))
            }

            #[allow(dead_code)]
            pub fn with<F: FnOnce() -> R, R>(value: $ty, f: F) -> ($ty, R) {
                let prev = set(Some(value));
                let res = f();
                let value = set(prev).unwrap();
                (value, res)
            }

            #[allow(dead_code)]
            pub fn try_borrow_with<F: FnOnce(&Option<$ty>) -> R, R>(f: F) -> R {
                SCOPE.with(|r| f(&*r.borrow()))
            }

            #[allow(dead_code)]
            pub fn try_borrow_mut_with<F: FnOnce(&mut Option<$ty>) -> R, R>(f: F) -> R {
                SCOPE.with(|r| f(&mut *r.borrow_mut()))
            }

            #[allow(dead_code)]
            pub fn borrow_with<F: FnOnce(&$ty) -> R, R>(f: F) -> R {
                SCOPE.with(|r| {
                    f(&*r.borrow().as_ref().expect(concat!(
                        "missing ",
                        module_path!(),
                        " in thread scope"
                    )))
                })
            }

            #[allow(dead_code)]
            pub fn borrow_mut_with<F: FnOnce(&mut $ty) -> R, R>(f: F) -> R {
                SCOPE.with(|r| {
                    f(&mut *r.borrow_mut().as_mut().expect(concat!(
                        "missing ",
                        module_path!(),
                        " in thread scope"
                    )))
                })
            }
        }
    };
}

pub use define;

#[cfg(test)]
mod tests {
    use super::*;

    define!(my_scope, u64);

    #[test]
    fn nested() {
        my_scope::try_borrow_with(|v| assert_eq!(*v, None));
        my_scope::with(123, || {
            my_scope::try_borrow_with(|v| assert_eq!(*v, Some(123)));
            my_scope::with(456, || {
                my_scope::try_borrow_with(|v| assert_eq!(*v, Some(456)));
            });
            my_scope::try_borrow_with(|v| assert_eq!(*v, Some(123)));
        });
        my_scope::try_borrow_with(|v| assert_eq!(*v, None));
    }
}
