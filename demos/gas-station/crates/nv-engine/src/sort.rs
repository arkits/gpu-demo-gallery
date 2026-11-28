//! The C++ library's `std::sort` as the viewers were built with it (Visual C++ 7.1's
//! Dinkumware introsort: `0x4698c0`, partition `0x4693b0`, insertion sort `0x469720`), so
//! that draws at equal distances come out in the same order. It is not stable.

const ISORT_MAX: usize = 32;

fn med3<T, F: Fn(&T, &T) -> bool>(v: &mut [T], a: usize, m: usize, z: usize, lt: &F) {
    if lt(&v[m], &v[a]) {
        v.swap(m, a);
    }
    if lt(&v[z], &v[m]) {
        v.swap(z, m);
    }
    if lt(&v[m], &v[a]) {
        v.swap(m, a);
    }
}

fn median<T, F: Fn(&T, &T) -> bool>(v: &mut [T], first: usize, mid: usize, last: usize, lt: &F) {
    if last - first > 40 {
        let step = (last - first + 1) / 8;
        med3(v, first, first + step, first + 2 * step, lt);
        med3(v, mid - step, mid, mid + step, lt);
        med3(v, last - 2 * step, last - step, last, lt);
        med3(v, first + step, mid, last - step, lt);
    } else {
        med3(v, first, mid, last, lt);
    }
}

fn partition<T, F: Fn(&T, &T) -> bool>(v: &mut [T], first: usize, last: usize, lt: &F) -> (usize, usize) {
    let mid = first + (last - first) / 2;
    median(v, first, mid, last - 1, lt);
    let mut pfirst = mid;
    let mut plast = pfirst + 1;
    while first < pfirst && !lt(&v[pfirst - 1], &v[pfirst]) && !lt(&v[pfirst], &v[pfirst - 1]) {
        pfirst -= 1;
    }
    while plast < last && !lt(&v[plast], &v[pfirst]) && !lt(&v[pfirst], &v[plast]) {
        plast += 1;
    }
    let mut gfirst = plast;
    let mut glast = pfirst;
    loop {
        while gfirst < last {
            if lt(&v[pfirst], &v[gfirst]) {
            } else if lt(&v[gfirst], &v[pfirst]) {
                break;
            } else {
                v.swap(plast, gfirst);
                plast += 1;
            }
            gfirst += 1;
        }
        while first < glast {
            if lt(&v[glast - 1], &v[pfirst]) {
            } else if lt(&v[pfirst], &v[glast - 1]) {
                break;
            } else {
                pfirst -= 1;
                v.swap(pfirst, glast - 1);
            }
            glast -= 1;
        }
        if glast == first && gfirst == last {
            return (pfirst, plast);
        }
        if glast == first {
            if plast != gfirst {
                v.swap(pfirst, plast);
            }
            plast += 1;
            v.swap(pfirst, gfirst);
            pfirst += 1;
            gfirst += 1;
        } else if gfirst == last {
            glast -= 1;
            pfirst -= 1;
            if glast != pfirst {
                v.swap(glast, pfirst);
            }
            plast -= 1;
            v.swap(pfirst, plast);
        } else {
            glast -= 1;
            v.swap(gfirst, glast);
            gfirst += 1;
        }
    }
}

fn insertion<T: Clone, F: Fn(&T, &T) -> bool>(v: &mut [T], first: usize, last: usize, lt: &F) {
    if first == last {
        return;
    }
    let mut next = first + 1;
    while next != last {
        let val = v[next].clone();
        if lt(&val, &v[first]) {
            // copy_backward(first, next, next + 1)
            let mut i = next;
            while i > first {
                v[i] = v[i - 1].clone();
                i -= 1;
            }
            v[first] = val;
        } else {
            let mut hole = next;
            while lt(&val, &v[hole - 1]) {
                v[hole] = v[hole - 1].clone();
                hole -= 1;
            }
            v[hole] = val;
        }
        next += 1;
    }
}

fn adjust_heap<T: Clone, F: Fn(&T, &T) -> bool>(v: &mut [T], first: usize, mut hole: usize, bottom: usize, val: T, lt: &F) {
    let top = hole;
    let mut idx = 2 * hole + 2;
    while idx < bottom {
        if lt(&v[first + idx], &v[first + idx - 1]) {
            idx -= 1;
        }
        v[first + hole] = v[first + idx].clone();
        hole = idx;
        idx = 2 * idx + 2;
    }
    if idx == bottom {
        v[first + hole] = v[first + bottom - 1].clone();
        hole = bottom - 1;
    }
    // _Push_heap
    let mut h = hole;
    while top < h {
        let parent = (h - 1) / 2;
        if !lt(&v[first + parent], &val) {
            break;
        }
        v[first + h] = v[first + parent].clone();
        h = parent;
    }
    v[first + h] = val;
}

fn heap_sort<T: Clone, F: Fn(&T, &T) -> bool>(v: &mut [T], first: usize, last: usize, lt: &F) {
    let n = last - first;
    // make_heap
    if n >= 2 {
        let mut hole = n / 2;
        while hole > 0 {
            hole -= 1;
            let val = v[first + hole].clone();
            adjust_heap(v, first, hole, n, val, lt);
        }
    }
    // sort_heap
    let mut end = last;
    while end - first > 1 {
        end -= 1;
        let val = v[end].clone();
        v[end] = v[first].clone();
        adjust_heap(v, first, 0, end - first, val, lt);
    }
}

fn sort_range<T: Clone, F: Fn(&T, &T) -> bool>(v: &mut [T], mut first: usize, mut last: usize, mut ideal: isize, lt: &F) {
    let mut count;
    loop {
        count = last - first;
        if !(ISORT_MAX < count && 0 < ideal) {
            break;
        }
        let (mf, ms) = partition(v, first, last, lt);
        ideal /= 2;
        ideal += ideal / 2;
        if mf - first < last - ms {
            sort_range(v, first, mf, ideal, lt);
            first = ms;
        } else {
            sort_range(v, ms, last, ideal, lt);
            last = mf;
        }
    }
    if ISORT_MAX < count {
        heap_sort(v, first, last, lt);
    } else if 1 < count {
        insertion(v, first, last, lt);
    }
}

/// `std::sort(v.begin(), v.end(), lt)`.
pub fn sort<T: Clone, F: Fn(&T, &T) -> bool>(v: &mut [T], lt: F) {
    let n = v.len();
    sort_range(v, 0, n, n as isize, &lt);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts() {
        let mut seed = 12345u32;
        for n in [0usize, 1, 2, 5, 31, 32, 33, 41, 100, 1000] {
            let mut v: Vec<(u32, usize)> = (0..n)
                .map(|i| {
                    seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                    ((seed >> 16) % 50, i)
                })
                .collect();
            sort(&mut v, |a, b| a.0 < b.0);
            assert!(v.windows(2).all(|w| w[0].0 <= w[1].0));
        }
    }
}
