use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use std::thread;

use std::io::{stdin, stdout, Write};

//generators implement this
trait Generator: Send {
    fn next_sample(&mut self, sample_rate: f32) -> f32;
    fn input_control(&mut self, inputs: Vec<f32>);
    fn get_id(&self) -> &String;
    fn get_out(&self) -> &String;
    fn set_out(&mut self, out: &String);
}

//possible things you can ask the audio thread to do
enum Instruction {
    NewGenerator(Box<dyn Generator>),
    DeleteGenerator(String),
    BindGenerator(String, String),
}

//simple sawtooth generator
//TODO: make it not go out of tune the higher it gets
#[derive(Clone)] //derive here just so that it can be vec'd and retained nicely, etc
struct Saw {
    frequency: f32,
    count: i32,
    val: f32,
    id: String,
    out: String,
}

impl Saw {
    pub fn new(frequency: f32, count: i32, val: f32, id: String) -> Self {
        //TODO fix indentation
        Self {
            frequency: frequency,
            count: count,
            val: val,
            id: id,
            out: String::new(),
        }
    }

    fn set_frequency(&mut self, freq: f32) {
        self.frequency = freq;
    }
}

impl Generator for Saw {
    fn next_sample(&mut self, sample_rate: f32) -> f32 {
        if self.count >= (sample_rate / self.frequency) as i32 {
            self.count = 0;
        } else {
            self.count += 1;
        }

        if self.count == 0 {
            self.val = 1.0;
        } else {
            self.val -= 1.0 / (sample_rate / self.frequency);
        }

        self.val - 0.5
    }

    fn input_control(&mut self, inputs: Vec<f32>) {}

    fn get_id(&self) -> &String {
        &self.id
    }

    fn get_out(&self) -> &String {
        &self.out
    }

    fn set_out(&mut self, out: &String) {
        self.out = out.clone();
        //self.out.pop();
    }
}

fn main() {
    //get a sender and receiver to send data to the audio thread and retrieve data in the audio thread
    let (command_sender, command_receiver): (
        crossbeam_channel::Sender<Instruction>,
        crossbeam_channel::Receiver<Instruction>,
    ) = crossbeam_channel::bounded(1024);

    //make a vector for threads running.
    let mut children = vec![];

    //make the audio thread!
    children.push(thread::spawn( move ||  {
    //ABANDON HOPE
    #[cfg(all(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd"), feature = "jack"))]

    //manually check for flags. can be passed through cargo with -- e.g.
    // cargo run --release --example beep --features jack -- --jack
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

    //run with the sample format given by the device's default output config
    match config.sample_format() {
        cpal::SampleFormat::F32 => run::<f32>(&device, &config.into(), command_receiver.clone()).unwrap(),
        cpal::SampleFormat::I16 => run::<i16>(&device, &config.into(), command_receiver.clone()).unwrap(),
        cpal::SampleFormat::U16 => run::<u16>(&device, &config.into(), command_receiver.clone()).unwrap(),
    };}));

    //RE-FIND YOUR HOPE
    loop {
        //grab user input
        let mut input = String::new();

        print!("> ");
        stdout().flush().expect("Error: Failed to flush stdout");

        stdin()
            .read_line(&mut input)
            .expect("Error: failed to read user input.");

        //perform some basic functions with this input
        //TODO: implement full parser/interpreter
        let input_parts: Vec<&str> = input.trim_end().split(" ").collect();

        if input_parts[0] == "new" {
            command_sender.send(Instruction::NewGenerator(Box::new(Saw::new(
                input_parts[1].parse::<f32>().expect("ERROR PROBABLY"),
                0,
                0.0,
                String::from(input_parts[2]),
            ))));
        } else if input_parts[0] == "del" {
            command_sender.send(Instruction::DeleteGenerator(String::from(input_parts[1])));
        } else if input_parts[0] == "bind" {
            command_sender.send(Instruction::BindGenerator(String::from(input_parts[1]), String::from(input_parts[2])));
        }
    }
}

//LOSE SOME OF THE HOPE YOU GOT BACK BUT NOT ALL (seriously it's not too bad)
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

    //make a nice list of generators
    //TODO: make the initialization be proper so that later on we're not allocating anything in the audio thread.
    let mut generators: Vec<Box<dyn Generator>> = Vec::new();

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            //for every buffer of audio sort of
            for frame in data.chunks_mut(channels) {
                let mut out: f32 = 0f32;

                let current_gen_count = generators.len(); //grab the count of generators here so that we're not doing dereferencing borrowing nonsense in the loop below

                let output_bus = String::from("out");

                //grab samples from all the generators
                for gen in &mut generators {
                    if (gen.get_out() == &output_bus) {
                        out += (gen.next_sample(sample_rate) / current_gen_count as f32) / 3f32; //just so that volume remains reasonable before proper volume stuff is implemented
                    }
                }

                let value: T = cpal::Sample::from::<f32>(&out); //make it into cpal's sample type

                //grab data from the main thread and perform some tasks based on that data.
                while let Ok(instruction) = command_receiver.try_recv() {
                    match instruction {
                        Instruction::NewGenerator(generator) => {
                            generators.push(generator);
                        }
                        Instruction::DeleteGenerator(id) => {
                            generators.retain(|x| *(x.get_id()) != id);
                        }
                        Instruction::BindGenerator(id_one, id_two) => {
                            for generator in &mut generators {
                                if generator.get_id() == &id_one {
                                    generator.set_out(&id_two);
                                }
                            }
                        }
                    }
                }

                //for every sample of this buffer (left and right samples simultaneously at the same time perhaps???)
                //TODO: understand how the frame works / how to access individual channels instead of just using copied and pasted code.
                for sample in frame.iter_mut() {
                    //make it the sample we got earlier
                    *sample = value
                }
            }
        },
        err_fn,
    )?;
    stream.play()?;

    //loops forever so the thread doesn't die immediately
    loop {}

    Ok(())
}
