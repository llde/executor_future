use std::cell::RefCell;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::mem::transmute;
use std::ops::Deref;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::thread::JoinHandle;

pub enum PollResult<T> {
    Polling,
    Ready(T),
}

pub trait OperationalPromise: Send + Sync {
    type Output;
    fn run(&self);
    fn poll(&self) -> PollResult<Self::Output>;
    fn get(&self) -> Self::Output {
        loop {
            match self.poll() {
                PollResult::Ready(ret) => break ret,
                PollResult::Polling => continue,
            }
        }
    }
 //   fn abort(&self);
}

pub trait Promise: Send + Sync + Clone {
    type Output;
    fn run(&self);
    fn poll(&self) -> PollResult<Self::Output>;
    fn get(&self) -> Self::Output {
        loop {
            match self.poll() {
                PollResult::Ready(ret) => break ret,
                PollResult::Polling => continue,
            }
        }
    }
  //  fn abort(&self);
}

impl<T> Promise for Arc<T>
where
    T: OperationalPromise,
{
    type Output = T::Output;
    fn run(&self) {
        self.deref().run();
    }
    fn poll(&self) -> PollResult<Self::Output> {
        self.deref().poll()
    }
    fn get(&self) -> Self::Output {
        self.deref().get()
    }
 //   fn abort(&self) {
   //     self.deref().abort();
    //}

}

pub struct Futurize<T> {
    inner: Arc<T>,
}

impl<T> Futurize<T> {
    pub fn new(to_inner: T) -> Futurize<T> {
        Futurize {
            inner: Arc::new(to_inner),
        }
    }
}

impl<T> Clone for Futurize<T> {
    fn clone(&self) -> Self {
        Futurize {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Promise for Futurize<T>
where
    T: OperationalPromise,
{
    type Output = T::Output;
    fn run(&self) {
        self.inner.run();
    }
    fn poll(&self) -> PollResult<Self::Output> {
        self.inner.poll()
    }
    fn get(&self) -> Self::Output {
        self.inner.get()
    }
 //   fn abort(&self) {
   //     self.inner.abort();
    //}
}

pub struct ThreadPoolExecutor<P>
where
    P: Promise,
{
    tasks: Arc<Mutex<VecDeque<P>>>,
    num_thread: u32,
    current_threads: Arc<RwLock<u32>>,
    handles: RefCell<Vec<JoinHandle<()>>>,
    ph: PhantomData<P>,
}

impl<P> ThreadPoolExecutor<P>
where
    P: Promise + 'static,
{
    pub fn new(num_threads: u32) -> ThreadPoolExecutor<P> {
        ThreadPoolExecutor {
            tasks: Arc::new(Mutex::new(VecDeque::new())),
            num_thread: num_threads,
            current_threads: Arc::new(RwLock::new(0)),
            handles: RefCell::new(Vec::new()),
            ph: PhantomData,
        }
    }

    pub fn submit<K: Promise>(&self, fut: &K) {
        let boxed = Box::new(fut.clone());
        let new_prom = unsafe { transmute::<Box<K>, Box<P>>(boxed) }; //We join the handlers for the thread at the end, so it's safe.
        self.tasks.lock().unwrap().push_back(*new_prom);
        //   println!("{} {}", *self.current_threads.read().unwrap(), self.num_thread);
        if *self.current_threads.read().unwrap() < self.num_thread {
            {
                let mut guard = self.current_threads.write().unwrap();
                *guard += 1;
            }
            let arc = self.tasks.clone();
            let arc_curr_thread = self.current_threads.clone();
            self.handles.borrow_mut().push(
                thread::Builder::new()
                    .name("Executor Thread".to_string())
                    .spawn(move || {
                        loop {
                            let option;
                            {
                                option = arc.lock().unwrap().pop_front();
                            }
                            if let Some(el) = option {
                                el.run();
                            } else {
                                break;
                            }
                        }
                        *arc_curr_thread.write().unwrap() -= 1;
                    })
                    .unwrap(),
            );
        }
    }
}

impl<P> Drop for ThreadPoolExecutor<P>
where
    P: Promise,
{
    fn drop(&mut self) {
        for i in self.handles.take() {
            if i.join().is_err() {
                panic!("Thread panicked");
            }
        }
    }
}
