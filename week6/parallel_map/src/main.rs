use crossbeam_channel;
use std::{thread, time};

fn parallel_map<T, U, F>(mut input_vec: Vec<T>, num_threads: usize, f: F) -> Vec<U>
where
    F: FnOnce(T) -> U + Send + Copy + 'static,
    T: Send + 'static,
    U: Send + 'static + Default,
{
    let mut output_vec: Vec<U> = Vec::with_capacity(input_vec.len());
    output_vec.resize_with(input_vec.len(), Default::default);

    let (input_sender, input_receiver) = crossbeam_channel::unbounded::<(usize, T)>();
    let (output_sender, output_receiver) = crossbeam_channel::unbounded::<(usize, U)>();
    let mut handles = vec![];

    while let Some(input) = input_vec.pop() {
        input_sender.send((input_vec.len(), input)).expect("wrong input sending");
    }
    drop(input_sender);

    for _ in 0..num_threads {
        let c_output_sender = output_sender.clone();
        let c_input_receiver = input_receiver.clone();
        let handle = thread::spawn(move || {
            while let Ok((counter, input)) = c_input_receiver.recv() {
                let res = f(input);
                c_output_sender.send((counter, res)).expect("wrong output sending");
            }
            drop(c_output_sender);
        });
        handles.push(handle);
    }
    drop(output_sender);

    for handle in handles {
        handle.join().unwrap();
    }
    while let Ok((index, output)) = output_receiver.recv() {
        output_vec[index] = output;
    }
    output_vec
}

fn main() {
    let v = vec![6, 7, 8, 9, 10, 1, 2, 3, 4, 5, 12, 18, 11, 5, 20];
    let squares = parallel_map(v, 10, |num| {
        println!("{} squared is {}", num, num * num);
        thread::sleep(time::Duration::from_millis(500));
        num * num
    });
    println!("squares: {:?}", squares);
}
