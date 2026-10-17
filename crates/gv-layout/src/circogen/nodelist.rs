//! `lib/circogen/nodelist.c`: operations on ordered node lists.

/// `appendNodelist(list, one, n)`: insert `n` at index `one`.
pub fn append_nodelist(list: &mut Vec<usize>, one: usize, n: usize) {
    assert!(one <= list.len());
    list.insert(one, n);
}

/// `realignNodelist(list, np)`: rotate so index `np` becomes the front.
pub fn realign_nodelist(list: &mut [usize], np: usize) {
    assert!(np < list.len());
    list.rotate_left(np);
}

/// `insertNodelist(list, cn, neighbor, pos)`: remove `cn`, then insert it
/// before `neighbor` if `pos == 0` and after it otherwise.
pub fn insert_nodelist(list: &mut Vec<usize>, cn: usize, neighbor: usize, pos: i32) {
    if let Some(i) = list.iter().position(|&x| x == cn) {
        list.remove(i);
    }
    if let Some(i) = list.iter().position(|&x| x == neighbor) {
        if pos == 0 {
            append_nodelist(list, i, cn);
        } else {
            append_nodelist(list, i + 1, cn);
        }
    }
}

/// `reverseAppend(l1, l2)`: `l1 @ rev(l2)`.
pub fn reverse_append(l1: &mut Vec<usize>, mut l2: Vec<usize>) {
    l2.reverse();
    l1.extend(l2);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_operations_match_c() {
        let mut l = vec![1, 2, 3, 4];
        append_nodelist(&mut l, 1, 9);
        assert_eq!(l, [1, 9, 2, 3, 4]);
        realign_nodelist(&mut l, 2);
        assert_eq!(l, [2, 3, 4, 1, 9]);
        insert_nodelist(&mut l, 9, 3, 0);
        assert_eq!(l, [2, 9, 3, 4, 1]);
        insert_nodelist(&mut l, 2, 1, 1);
        assert_eq!(l, [9, 3, 4, 1, 2]);
        reverse_append(&mut l, vec![7, 8]);
        assert_eq!(l, [9, 3, 4, 1, 2, 8, 7]);
    }
}
