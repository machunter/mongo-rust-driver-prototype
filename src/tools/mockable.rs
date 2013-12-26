/* Copyright 2013 10gen Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

 extern mod extra;
 use extra::future;
 use std::rt::uv::uvll;


pub trait Mockable  {
    fn mock(state: int) -> Self;
}

impl Mockable for () {
    fn mock(_: int) -> () { () }
}
impl Mockable for char {
    fn mock(_: int) -> char { 0u8 as char }
}

impl Mockable for int {
    fn mock(_: int) -> int { 0 }
}

impl Mockable for i8 {
    fn mock(_: int) -> i8 { 0i8}
}

impl Mockable for uint {
    fn mock(_: int) -> uint { 0u }
}

impl Mockable for u8 {
    fn mock(_: int) -> u8 { 0u8 }
}

impl Mockable for float {
    fn mock(_: int) -> float { 0f }
}

impl Mockable for ~str {
    fn mock(_: int) -> ~str { ~"" }
}

impl<T:Mockable> Mockable for ~T {
    fn mock(state: int) -> ~T {
        ~Mockable::mock(state)
    }
}

impl<T:Mockable> Mockable for @T {
    fn mock(state: int) -> @T {
        Mockable::mock(state)
    }
}

impl<T:Mockable> Mockable for ~[T] {
    fn mock(state: int) -> ~[T] {
        ~[Mockable::mock(state)]
    }
}

impl<T:Mockable> Mockable for Option<T> {
    fn mock(state: int) -> Option<T> {
        if state == 0 {
            Some(Mockable::mock(state))
        }
        else {
            None
        }
    }
}

impl<T:Mockable,U:Mockable> Mockable for Result<T,U> {
    fn mock(state: int) -> Result<T,U> {
        if state == 0 {
            Ok(Mockable::mock(state))
        }
        else if state == 1 {
            Err(Mockable::mock(state))
        }
        else {
            fail!("mocking error: invalid state from Result")
        }
    }
}

impl<T:Mockable + Send> Mockable for extra::future::Future<T> {
    fn mock(state: int) -> extra::future::Future<T> {
        do future::Future::spawn { Mockable::mock(state)}
    }
}

impl Mockable for uvll::uv_err_data  {
    fn mock(_: int) -> uvll::uv_err_data  {
        uvll::uv_err_data { err_name: ~"mock error", err_msg: ~"mock" }
    }
}
