pub mod generator;
pub mod generators;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::Sender;
use generator::GeneratorWrapper;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::{stdin, stdout, Write};
use std::thread;

use generators::saw::SawGenerator;

// Possible things you can ask the audio thread to do
enum Instruction {
    NewGenerator(GeneratorWrapper),
    DeleteGenerator(String),
    BindGenerator(String, String),
}

fn calculate_sample(
    wrappers: &mut HashMap<String, GeneratorWrapper>,
    sample_cache: &mut HashMap<String, Option<f32>>,
    id: String,
    sample_rate: f32,
) -> f32 {
    let mut out = 0.0;

    let wrapper = wrappers.get(&id).unwrap();
    for dependency in &wrapper.dependencies {
        if let Some(Some(sample)) = sample_cache.get(dependency) {
            out += *sample;
        } else {
            let sample = wrappers
                .get(dependency)
                .expect("hey told you this would panic, have fun :)")
                .generator
                .borrow_mut()
                .next_sample(sample_rate);
            sample_cache.insert(id.to_string(), Some(sample));
            out += sample;
        }
    }

    out / wrappers.len() as f32
}

fn handle_input(input: String, command_sender: &Sender<Instruction>) {
    // Perform some basic functions with this input
    // TODO: implement full parser/interpreter
    let input_parts: Vec<&str> = input.trim_end().split(" ").collect();

    match input_parts[0] {
        "new" => {
            let generator = SawGenerator::new(input_parts[1].parse::<f32>().unwrap());
            let wrapper = GeneratorWrapper::new(input_parts[2], Box::new(RefCell::new(generator)));

            command_sender
                .send(Instruction::NewGenerator(wrapper))
                .unwrap();
        }

        "del" => {
            command_sender
                .send(Instruction::DeleteGenerator(String::from(input_parts[1])))
                .unwrap();
        }

        "bind" => {
            command_sender
                .send(Instruction::BindGenerator(
                    String::from(input_parts[1]),
                    String::from(input_parts[2]),
                ))
                .unwrap();
        }

        _ => {}
    }
}

pub fn main_loop() {
    // Get a sender and receiver to send data to the audio thread and retrieve data in the audio thread
    let (command_sender, command_receiver): (
        crossbeam_channel::Sender<Instruction>,
        crossbeam_channel::Receiver<Instruction>,
    ) = crossbeam_channel::bounded(1024);

    // Make the audio thread!
    thread::spawn(move || {
        // ABANDON HOPE

        #[cfg(all(
            any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd"),
            feature = "jack"
        ))]
        let host = if std::env::args()
            .collect::<String>()
            .contains(&String::from("--jack"))
        {
            cpal::host_from_id(cpal::available_hosts()
                .into_iter()
                .find(|id| *id == cpal::HostId::Jack)
                .expect(
                    "make sure --features jack is specified. only works on OSes where jack is available",
                )).expect("jack host unavailable")
        } else {
            cpal::default_host()
        };

        #[cfg(any(
            not(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd")),
            not(feature = "jack")
        ))]
        let host = cpal::default_host();

        let device = host
            .default_output_device()
            .expect("failed to find a default output device");
        let config = device.default_output_config().unwrap();

        // Run with the sample format given by the device's default output config
        match config.sample_format() {
            cpal::SampleFormat::F32 => {
                run::<f32>(&device, &config.into(), command_receiver.clone()).unwrap()
            }
            cpal::SampleFormat::I16 => {
                run::<i16>(&device, &config.into(), command_receiver.clone()).unwrap()
            }
            cpal::SampleFormat::U16 => {
                run::<u16>(&device, &config.into(), command_receiver.clone()).unwrap()
            }
        };
    });

    // RE-FIND YOUR HOPE
    loop {
        // Grab user input
        let mut input = String::new();

        print!("> ");
        stdout().flush().expect("Error: Failed to flush stdout");

        stdin()
            .read_line(&mut input)
            .expect("Error: failed to read user input.");

        handle_input(input, &command_sender);
    }
}

// LOSE SOME OF THE HOPE YOU GOT BACK BUT NOT ALL (seriously it's not too bad)
fn run<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    command_receiver: crossbeam_channel::Receiver<Instruction>,
) -> Result<(), anyhow::Error>
where
    T: cpal::Sample,
{
    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;

    let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

    let mut wrappers: HashMap<String, GeneratorWrapper> = HashMap::new();
    let mut sample_cache: HashMap<String, Option<f32>> = HashMap::new();
    let mut out_dependencies: HashSet<String> = HashSet::new();

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            // For every buffer of audio sort of
            for frame in data.chunks_mut(channels) {
                let mut out: f32 = 0.0;
                for dependency in &out_dependencies {
                    out += calculate_sample(
                        &mut wrappers,
                        &mut sample_cache,
                        dependency.clone(),
                        sample_rate,
                    );
                }
                out /= out_dependencies.len() as f32;

                let value: T = cpal::Sample::from::<f32>(&out); // Make it into cpal's sample type

                // Grab data from the main thread and perform some tasks based on that data.
                while let Ok(instruction) = command_receiver.try_recv() {
                    match instruction {
                        Instruction::NewGenerator(generator) => {
                            let id = generator.get_id();
                            wrappers.insert(id.clone(), generator);
                            sample_cache.insert(id, None);
                        }
                        Instruction::DeleteGenerator(id) => {
                            wrappers.remove(&id);
                            sample_cache.remove(&id);
                        }
                        Instruction::BindGenerator(id_one, id_two) => {
                            if id_two == "out".to_string() {
                                out_dependencies.insert(id_one);
                            } else if let Some(wrapper) = wrappers.get_mut(&id_two) {
                                wrapper.dependencies.insert(id_one);
                            }
                        }
                    }
                }

                // For every sample of this buffer (left and right samples simultaneously at the same time perhaps???)
                // TODO: understand how the frame works / how to access individual channels instead of just using copied and pasted code.
                for sample in frame.iter_mut() {
                    // Make it the sample we got earlier
                    *sample = value
                }
            }
        },
        err_fn,
    )?;
    stream.play()?;

    // Loops forever so the thread doesn't die immediately
    loop {}
}
