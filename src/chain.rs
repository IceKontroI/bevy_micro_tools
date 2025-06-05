use sealed::Len;
use seq_macro::seq;

// TODO often we don't want to be able to transform inputs -> outputs during the chain
//      it would be helpful for the user to be able to implement a Sequence<N> wrapper
//      this would wrap the Chain<N> implementation, forcing its `In` and `Out` to be the same value

mod sealed {
    pub trait Len {
        const LEN: usize;
    }
}

// TODO I really hate this L<N> requirement, but we seem to need it to get around rust's
//      compiler bug where non-overlapping trait impls are detected as overlapping, when
pub struct L<const N: usize>;
impl<const N: usize> Len for L<N> {
    const LEN: usize = N;
}

pub trait Length {
    type Len: sealed::Len;

    fn len() -> usize {
        <Self::Len as Len>::LEN
    }
}

pub trait InRange<const N: usize, L>: Length {}

seq!(N in 1..=16 {
    seq!(I in 0..N {
        impl<T: Length<Len = L<N>>> InRange<I, L<N>> for T {}
    });
});

pub trait Chain<const N: usize>
where
    Self: Length,
    Self: InRange<N, <Self as Length>::Len>,
{
    type In<'a>;
    type Out<'a>;

    fn chain(input: Self::In<'_>) -> Self::Out<'_>;
}

pub trait Link<const N: usize> {
    type In<'a>;
    type Out<'a>;

    fn link<'a>(input: Self::In<'a>) -> Self::Out<'a>;
}

impl<T: Chain<0>> Link<1> for T {
    type In<'a> = <T as Chain<0>>::In<'a>;
    type Out<'a> = <T as Chain<0>>::Out<'a>;

    fn link(input: Self::In<'_>) -> Self::Out<'_> {
        return <T as Chain<0>>::chain(input);
    } 
}

seq!(N in 2..=16 {
    impl<T> Link<N> for T
    where
        T: Chain<0>,
        for<'a> T: Link<{N - 1}, In<'a> = <T as Chain<0>>::In<'a>>,
        for<'a> T: Chain<{N - 1}, In<'a> = <T as Link<{N - 1}>>::Out<'a>>,
    {
        type In<'a> = <T as Chain<0>>::In<'a>;
        type Out<'a> = <T as Chain<{N - 1}>>::Out<'a>;

        fn link(input: Self::In<'_>) -> Self::Out<'_> {
            let out = <T as Link<{N - 1}>>::link(input);
            return <T as Chain<{N - 1}>>::chain(out);
        }
    }
});

pub trait Cascade {
    type In<'a>;
    type Out<'a>;

    fn cascade(input: Self::In<'_>) -> Self::Out<'_>;
}

impl<const N: usize, T: Link<N> + Length<Len = L<N>>> Cascade for T {
    type In<'a> = T::In<'a>;
    type Out<'a> = T::Out<'a>;

    fn cascade(input: Self::In<'_>) -> Self::Out<'_> {
        return <T as Link::<N>>::link(input);
    }
}

#[cfg(test)]
mod chain_test {
    
    use super::*;
    
    #[test]
    fn testing() {

        struct ChainTest;

        impl Chain<0> for ChainTest {
            type In<'a> = f32;
            type Out<'a> = i32;

            fn chain(input: Self::In<'_>) -> Self::Out<'_> {
                let output = input as i32;
                println!("Chain 0: {input} -> {output}");
                output
            }
        }

        impl Chain<1> for ChainTest {
            type In<'a> = i32;
            type Out<'a> = u32;

            fn chain(input: Self::In<'_>) -> Self::Out<'_> {
                let output = input as u32;
                println!("Chain 1: {input} -> {output}");
                output
            }
        }

        impl Chain<2> for ChainTest {
            type In<'a> = u32;
            type Out<'a> = String;

            fn chain(input: Self::In<'_>) -> Self::Out<'_> {
                let mut output = String::new();
                let mut n = input;
                while n >= 1_000 {
                    output = format!(",{:03}{}", n % 1_000, output);
                    n /= 1_000;
                }
                println!("Chain 2: {input} -> {n}{output}");
                output
            }
        }

        impl Length for ChainTest {
            type Len = L<3>;
        }

        ChainTest::cascade(-1.5);
    }
}
