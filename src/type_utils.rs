macro_rules! for_each_function_signature {
    ($mac:ident) => {
        $mac!(0);
        $mac!(1 A1);
        $mac!(2 A1 A2);
        $mac!(3 A1 A2 A3);
        $mac!(4 A1 A2 A3 A4);
        $mac!(5 A1 A2 A3 A4 A5);
        $mac!(6 A1 A2 A3 A4 A5 A6);
        $mac!(7 A1 A2 A3 A4 A5 A6 A7);
        $mac!(8 A1 A2 A3 A4 A5 A6 A7 A8);
        $mac!(9 A1 A2 A3 A4 A5 A6 A7 A8 A9);
        $mac!(10 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10);
        $mac!(11 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10 A11);
        $mac!(12 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10 A11 A12);
        $mac!(13 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10 A11 A12 A13);
        $mac!(14 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10 A11 A12 A13 A14);
        $mac!(15 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10 A11 A12 A13 A14 A15);
        $mac!(16 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10 A11 A12 A13 A14 A15 A16);
    };
}

pub trait IOToFn {
    type Output;
}

macro_rules! impl_io_to_fn {
    ($num:tt $arg:ident) => {
        #[allow(non_snake_case)]
        impl<$arg, Res> IOToFn for (($arg,), Res)
        {
            type Output = unsafe extern "C" fn($arg) -> Res;
        }
    };
    ($num:tt $($args:ident)*) => {
        #[allow(non_snake_case)]
        impl<$($args,)* Res> IOToFn for (($($args,)*), Res)
        {
            type Output = unsafe extern "C" fn($($args),*) -> Res;
        }
    }
}

for_each_function_signature!(impl_io_to_fn);

pub trait Caller<Args, Res> where (Args, Res): IOToFn {
    fn call(input: Args, func: &<(Args, Res) as IOToFn>::Output) -> Res;
}

macro_rules! impl_caller {
    ($num:tt $arg:ident) => {
        #[allow(non_snake_case)]
        impl<$arg, Res> Caller<($arg,), Res> for ($arg,)
        {
            fn call(($arg,): ($arg,), func: &unsafe extern "C" fn($arg) -> Res) -> Res {
                unsafe { func($arg) }
            }
        }
    };
    ($num:tt $($args:ident)*) => {
        #[allow(non_snake_case)]
        impl<$($args,)* Res> Caller<($($args,)*), Res> for ($($args,)*)
        {
            fn call(($($args,)*): ($($args,)*), func: &unsafe extern "C" fn($($args),*) -> Res) -> Res {
                unsafe { func($($args),*) }
            }
        }
    }
}

for_each_function_signature!(impl_caller);
