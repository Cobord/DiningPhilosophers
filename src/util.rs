use std::mem;

use nonempty::NonEmpty;

// copied from a PR on nonempty but not in release yet

/// Sorts the nonempty.
pub(crate) fn nonempty_sort<T>(me: &mut NonEmpty<T>)
where
    T: Ord,
{
    me.tail.sort();
    let index = match me.tail.binary_search(&me.head) {
        Ok(index) | Err(index) => index,
    };

    if index != 0 {
        let new_head = me.tail.remove(0);
        let head = mem::replace(&mut me.head, new_head);
        me.tail.insert(index - 1, head);
    }
}
