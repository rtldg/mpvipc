pub mod ipc;

use ipc::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt::{self, Display};
use std::io::BufReader;
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle};
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};
use interprocess::local_socket::LocalSocketStream;

#[derive(Debug)]
pub enum Event {
    Shutdown,
    StartFile,
    EndFile,
    FileLoaded,
    TracksChanged,
    TrackSwitched,
    Idle,
    Pause,
    Unpause,
    Tick,
    VideoReconfig,
    AudioReconfig,
    MetadataUpdate,
    Seek,
    PlaybackRestart,
    PropertyChange { id: usize, property: Property },
    ChapterChange,
    ClientMessage { args: Vec<String> },
    Unimplemented,
}

#[derive(Debug)]
pub enum Property {
    Path(Option<String>),
    Pause(bool),
    PlaybackTime(Option<f64>),
    Duration(Option<f64>),
    Metadata(Option<HashMap<String, MpvDataType>>),
    Unknown { name: String, data: MpvDataType },
}

pub enum MpvCommand {
    LoadFile {
        file: String,
        option: PlaylistAddOptions,
    },
    LoadList {
        file: String,
        option: PlaylistAddOptions,
    },
    PlaylistClear,
    PlaylistMove {
        from: usize,
        to: usize,
    },
    Observe {
        id: isize,
        property: String,
    },
    PlaylistNext,
    PlaylistPrev,
    PlaylistRemove(usize),
    PlaylistShuffle,
    Quit,
    ScriptMessage(Vec<String>),
    ScriptMessageTo {
        target: String,
        args: Vec<String>,
    },
    Seek {
        seconds: f64,
        option: SeekOptions,
    },
    Stop,
    Unobserve(isize),
    ShowText {
        text: String,
        duration_ms: Option<i32>,
        level: Option<u32>,
    },
}

#[derive(Debug)]
pub enum MpvDataType {
    Array(Vec<MpvDataType>),
    Bool(bool),
    Double(f64),
    HashMap(HashMap<String, MpvDataType>),
    Null,
    Playlist(Playlist),
    String(String),
    Usize(usize),
}

pub enum NumberChangeOptions {
    Absolute,
    Increase,
    Decrease,
}

pub enum PlaylistAddOptions {
    Replace,
    Append,
}

pub enum PlaylistAddTypeOptions {
    File,
    Playlist,
}

pub enum SeekOptions {
    Relative,
    Absolute,
    RelativePercent,
    AbsolutePercent,
}

pub enum Switch {
    On,
    Off,
    Toggle,
}

#[derive(Debug)]
pub enum ErrorCode {
    MpvError(String),
    JsonParseError(String),
    ConnectError(String),
    JsonContainsUnexptectedType,
    UnexpectedResult,
    UnexpectedValue,
    MissingValue,
    UnsupportedType,
    ValueDoesNotContainBool,
    ValueDoesNotContainF64,
    ValueDoesNotContainHashMap,
    ValueDoesNotContainPlaylist,
    ValueDoesNotContainString,
    ValueDoesNotContainUsize,
}

pub struct Mpv {
    stream: LocalSocketStream,
    reader: BufReader<LocalSocketStream>,
    name: String,
}
#[derive(Debug)]
pub struct Playlist(pub Vec<PlaylistEntry>);
#[derive(Debug)]
pub struct Error(pub ErrorCode);

impl Drop for Mpv {
    fn drop(&mut self) {
        self.disconnect();
    }
}

impl fmt::Debug for Mpv {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_tuple("Mpv").field(&self.name).finish()
    }
}

#[allow(non_snake_case)]
pub fn clone_LocalSocketStream(stream: &LocalSocketStream) -> LocalSocketStream {
    #[cfg(windows)]
    unsafe { LocalSocketStream::from_raw_handle(stream.as_raw_handle()) }
    #[cfg(unix)]
    unsafe { LocalSocketStream::from_raw_fd(stream.as_raw_fd()) }
}

// unsafe
impl Clone for Mpv {
    fn clone(&self) -> Self {
        let stream = clone_LocalSocketStream(&self.stream);
        let cloned_stream = clone_LocalSocketStream(&self.stream);
        Mpv {
            stream,
            reader: BufReader::new(cloned_stream),
            name: self.name.clone(),
        }
    }

    fn clone_from(&mut self, source: &Self) {
        let stream = clone_LocalSocketStream(&self.stream);
        let cloned_stream = clone_LocalSocketStream(&self.stream);
        *self = Mpv {
            stream,
            reader: BufReader::new(cloned_stream),
            name: source.name.clone(),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl std::error::Error for Error {}

impl Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ErrorCode::ConnectError(ref msg) => f.write_str(&format!("ConnectError: {}", msg)),
            ErrorCode::JsonParseError(ref msg) => f.write_str(&format!("JsonParseError: {}", msg)),
            ErrorCode::MpvError(ref msg) => f.write_str(&format!("MpvError: {}", msg)),
            ErrorCode::JsonContainsUnexptectedType => {
                f.write_str("Mpv sent a value with an unexpected type")
            }
            ErrorCode::UnexpectedResult => f.write_str("Unexpected result received"),
            ErrorCode::UnexpectedValue => f.write_str("Unexpected value received"),
            ErrorCode::MissingValue => f.write_str("Missing value"),
            ErrorCode::UnsupportedType => f.write_str("Unsupported type received"),
            ErrorCode::ValueDoesNotContainBool => {
                f.write_str("The received value is not of type \'std::bool\'")
            }
            ErrorCode::ValueDoesNotContainF64 => {
                f.write_str("The received value is not of type \'std::f64\'")
            }
            ErrorCode::ValueDoesNotContainHashMap => {
                f.write_str("The received value is not of type \'std::collections::HashMap\'")
            }
            ErrorCode::ValueDoesNotContainPlaylist => {
                f.write_str("The received value is not of type \'mpvipc::Playlist\'")
            }
            ErrorCode::ValueDoesNotContainString => {
                f.write_str("The received value is not of type \'std::string::String\'")
            }
            ErrorCode::ValueDoesNotContainUsize => {
                f.write_str("The received value is not of type \'std::usize\'")
            }
        }
    }
}

pub trait GetPropertyTypeHandler: Sized {
    fn get_property_generic(instance: &Mpv, property: &str) -> Result<Self, Error>;
}

impl GetPropertyTypeHandler for bool {
    fn get_property_generic(instance: &Mpv, property: &str) -> Result<bool, Error> {
        get_mpv_property::<bool>(instance, property)
    }
}

impl GetPropertyTypeHandler for String {
    fn get_property_generic(instance: &Mpv, property: &str) -> Result<String, Error> {
        get_mpv_property::<String>(instance, property)
    }
}

impl GetPropertyTypeHandler for f64 {
    fn get_property_generic(instance: &Mpv, property: &str) -> Result<f64, Error> {
        get_mpv_property::<f64>(instance, property)
    }
}

impl GetPropertyTypeHandler for usize {
    fn get_property_generic(instance: &Mpv, property: &str) -> Result<usize, Error> {
        get_mpv_property::<usize>(instance, property)
    }
}

impl GetPropertyTypeHandler for Vec<PlaylistEntry> {
    fn get_property_generic(instance: &Mpv, property: &str) -> Result<Vec<PlaylistEntry>, Error> {
        get_mpv_property::<Vec<PlaylistEntry>>(instance, property)
    }
}

impl GetPropertyTypeHandler for HashMap<String, MpvDataType> {
    fn get_property_generic(
        instance: &Mpv,
        property: &str,
    ) -> Result<HashMap<String, MpvDataType>, Error> {
        get_mpv_property::<HashMap<String, MpvDataType>>(instance, property)
    }
}

pub trait SetPropertyTypeHandler<T> {
    fn set_property_generic(instance: &Mpv, property: &str, value: T) -> Result<(), Error>;
}

impl SetPropertyTypeHandler<bool> for bool {
    fn set_property_generic(instance: &Mpv, property: &str, value: bool) -> Result<(), Error> {
        set_mpv_property(instance, property, json!(value))
    }
}

impl SetPropertyTypeHandler<String> for String {
    fn set_property_generic(instance: &Mpv, property: &str, value: String) -> Result<(), Error> {
        set_mpv_property(instance, property, json!(value))
    }
}

impl SetPropertyTypeHandler<f64> for f64 {
    fn set_property_generic(instance: &Mpv, property: &str, value: f64) -> Result<(), Error> {
        set_mpv_property(instance, property, json!(value))
    }
}

impl SetPropertyTypeHandler<usize> for usize {
    fn set_property_generic(instance: &Mpv, property: &str, value: usize) -> Result<(), Error> {
        set_mpv_property(instance, property, json!(value))
    }
}

impl Mpv {
    pub fn connect(socket: &str) -> Result<Mpv, Error> {
        match LocalSocketStream::connect(socket) {
            Ok(stream) => {
                let cloned_stream = clone_LocalSocketStream(&stream);
                return Ok(Mpv {
                    stream,
                    reader: BufReader::new(cloned_stream),
                    name: String::from(socket),
                });
            }
            Err(internal_error) => Err(Error(ErrorCode::ConnectError(internal_error.to_string()))),
        }
    }

    pub fn disconnect(&self) {
        /*
        let mut stream = &self.stream;
        stream
            .shutdown(std::net::Shutdown::Both)
            .expect("socket disconnect");
        let mut buffer = [0; 32];
        for _ in 0..stream.bytes().count() {
            stream.read(&mut buffer[..]).unwrap();
        }
        */
    }

    pub fn get_stream_ref(&self) -> &LocalSocketStream {
        &self.stream
    }

    pub fn get_metadata(&self) -> Result<HashMap<String, MpvDataType>, Error> {
        match get_mpv_property(self, "metadata") {
            Ok(map) => Ok(map),
            Err(err) => Err(err),
        }
    }

    pub fn get_playlist(&self) -> Result<Playlist, Error> {
        match get_mpv_property::<Vec<PlaylistEntry>>(self, "playlist") {
            Ok(entries) => Ok(Playlist(entries)),
            Err(msg) => Err(msg),
        }
    }

    /// # Description
    ///
    /// Retrieves the property value from mpv.
    ///
    /// ## Supported types
    /// - String
    /// - bool
    /// - HashMap<String, String> (e.g. for the 'metadata' property)
    /// - Vec<PlaylistEntry> (for the 'playlist' property)
    /// - usize
    /// - f64
    ///
    /// ## Input arguments
    ///
    /// - **property** defines the mpv property that should be retrieved
    ///
    /// # Example
    /// ```
    /// use mpvipc::{Mpv, Error};
    /// fn main() -> Result<(), Error> {
    ///     let mpv = Mpv::connect("/tmp/mpvsocket")?;
    ///     let paused: bool = mpv.get_property("pause")?;
    ///     let title: String = mpv.get_property("media-title")?;
    ///     Ok(())
    /// }
    /// ```
    pub fn get_property<T: GetPropertyTypeHandler>(&self, property: &str) -> Result<T, Error> {
        T::get_property_generic(self, property)
    }

    /// # Description
    ///
    /// Retrieves the property value from mpv.
    /// The result is always of type String, regardless of the type of the value of the mpv property
    ///
    /// ## Input arguments
    ///
    /// - **property** defines the mpv property that should be retrieved
    ///
    /// # Example
    ///
    /// ```
    /// use mpvipc::{Mpv, Error};
    /// fn main() -> Result<(), Error> {
    ///     let mpv = Mpv::connect("/tmp/mpvsocket")?;
    ///     let title = mpv.get_property_string("media-title")?;
    ///     Ok(())
    /// }
    /// ```
    pub fn get_property_string(&self, property: &str) -> Result<String, Error> {
        get_mpv_property_string(self, property)
    }

    pub fn kill(&self) -> Result<(), Error> {
        self.run_command(MpvCommand::Quit)
    }

    /// # Description
    ///
    /// Waits until an mpv event occurs and returns the Event.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut mpv = Mpv::connect("/tmp/mpvsocket")?;
    /// loop {
    ///     let event = mpv.event_listen()?;
    ///     println!("{:?}", event);
    /// }
    /// ```
    pub fn event_listen(&mut self) -> Result<Event, Error> {
        listen(self)
    }

    pub fn event_listen_raw(&mut self) -> String {
        listen_raw(self)
    }

    pub fn next(&self) -> Result<(), Error> {
        self.run_command(MpvCommand::PlaylistNext)
    }

    pub fn observe_property(&self, id: isize, property: &str) -> Result<(), Error> {
        self.run_command(MpvCommand::Observe {
            id: id,
            property: property.to_string(),
        })
    }

    pub fn unobserve_property(&self, id: isize) -> Result<(), Error> {
        self.run_command(MpvCommand::Unobserve(id))
    }

    pub fn pause(&self) -> Result<(), Error> {
        set_mpv_property(self, "pause", json!(true))
    }

    pub fn prev(&self) -> Result<(), Error> {
        self.run_command(MpvCommand::PlaylistPrev)
    }

    pub fn restart(&self) -> Result<(), Error> {
        self.run_command(MpvCommand::Seek {
            seconds: 0f64,
            option: SeekOptions::Absolute,
        })
    }

    /// # Description
    ///
    /// Runs mpv commands. The arguments are passed as a String-Vector reference:
    ///
    /// ## Input arguments
    ///
    /// - **command**   defines the mpv command that should be executed
    /// - **args**      a slice of &str's which define the arguments
    ///
    /// # Example
    /// ```
    /// use mpvipc::{Mpv, Error};
    /// fn main() -> Result<(), Error> {
    ///     let mpv = Mpv::connect("/tmp/mpvsocket")?;
    ///
    ///     //Run command 'playlist-shuffle' which takes no arguments
    ///     mpv.run_command(MpvCommand::PlaylistShuffle)?;
    ///
    ///     //Run command 'seek' which in this case takes two arguments
    ///     mpv.run_command(MpvCommand::Seek {
    ///         seconds: 0f64,
    ///         option: SeekOptions::Absolute,
    ///     })?;
    ///     Ok(())
    /// }
    /// ```
    pub fn run_command(&self, command: MpvCommand) -> Result<(), Error> {
        match command {
            MpvCommand::LoadFile { file, option } => run_mpv_command(
                self,
                "loadfile",
                &[
                    file.as_ref(),
                    match option {
                        PlaylistAddOptions::Append => "append",
                        PlaylistAddOptions::Replace => "replace",
                    },
                ],
            ),
            MpvCommand::LoadList { file, option } => run_mpv_command(
                self,
                "loadlist",
                &[
                    file.as_ref(),
                    match option {
                        PlaylistAddOptions::Append => "append",
                        PlaylistAddOptions::Replace => "replace",
                    },
                ],
            ),
            MpvCommand::Observe { id, property } => observe_mpv_property(self, &id, &property),
            MpvCommand::PlaylistClear => run_mpv_command(self, "playlist-clear", &[]),
            MpvCommand::PlaylistMove { from, to } => {
                run_mpv_command(self, "playlist-move", &[&from.to_string(), &to.to_string()])
            }
            MpvCommand::PlaylistNext => run_mpv_command(self, "playlist-next", &[]),
            MpvCommand::PlaylistPrev => run_mpv_command(self, "playlist-prev", &[]),
            MpvCommand::PlaylistRemove(id) => {
                run_mpv_command(self, "playlist-remove", &[&id.to_string()])
            }
            MpvCommand::PlaylistShuffle => run_mpv_command(self, "playlist-shuffle", &[]),
            MpvCommand::Quit => run_mpv_command(self, "quit", &[]),
            MpvCommand::ScriptMessage(args) => {
                let str_args: Vec<_> = args.iter().map(String::as_str).collect();
                run_mpv_command(self, "script-message", &str_args)
            }
            MpvCommand::ScriptMessageTo { target, args } => {
                let mut cmd_args: Vec<_> = vec![target.as_str()];
                let mut str_args: Vec<_> = args.iter().map(String::as_str).collect();
                cmd_args.append(&mut str_args);
                run_mpv_command(self, "script-message-to", &cmd_args)
            }
            MpvCommand::Seek { seconds, option } => run_mpv_command(
                self,
                "seek",
                &[
                    &seconds.to_string(),
                    match option {
                        SeekOptions::Absolute => "absolute",
                        SeekOptions::Relative => "relative",
                        SeekOptions::AbsolutePercent => "absolute-percent",
                        SeekOptions::RelativePercent => "relative-percent",
                    },
                ],
            ),
            MpvCommand::Stop => run_mpv_command(self, "stop", &[]),
            MpvCommand::Unobserve(id) => unobserve_mpv_property(self, &id),
            MpvCommand::ShowText { text, duration_ms, level }=> {
                let mut args = vec![text, duration_ms.unwrap_or(-1).to_string()];
                if let Some(level) = level {
                    args.push(level.to_string());
                }
                let str_args: Vec<_> = args.iter().map(String::as_str).collect();
                run_mpv_command(self, "show-text", &str_args)
            },
        }
    }

    /// Run a custom command.
    /// This should only be used if the desired command is not implemented
    /// with [MpvCommand].
    pub fn run_command_raw(&self, command: &str, args: &[&str]) -> Result<(), Error> {
        run_mpv_command(self, command, args)
    }
    /// Run a custom command.
    /// This should only be used if the desired command is not implemented
    /// with [MpvCommand].
    /// For when you want to pass [Input Command Prefixes](https://mpv.io/manual/master/#input-command-prefixes).
    pub fn run_command_raw2(&self, command: &[&str], args: &[&str]) -> Result<(), Error> {
        run_mpv_command2(self, command, args)
    }

    pub fn playlist_add(
        &self,
        file: &str,
        file_type: PlaylistAddTypeOptions,
        option: PlaylistAddOptions,
    ) -> Result<(), Error> {
        match file_type {
            PlaylistAddTypeOptions::File => self.run_command(MpvCommand::LoadFile {
                file: file.to_string(),
                option,
            }),

            PlaylistAddTypeOptions::Playlist => self.run_command(MpvCommand::LoadList {
                file: file.to_string(),
                option,
            }),
        }
    }

    pub fn playlist_clear(&self) -> Result<(), Error> {
        self.run_command(MpvCommand::PlaylistClear)
    }

    pub fn playlist_move_id(&self, from: usize, to: usize) -> Result<(), Error> {
        self.run_command(MpvCommand::PlaylistMove { from, to })
    }

    pub fn playlist_play_id(&self, id: usize) -> Result<(), Error> {
        set_mpv_property(self, "playlist-pos", json!(id))
    }

    pub fn playlist_play_next(&self, id: usize) -> Result<(), Error> {
        match get_mpv_property::<usize>(self, "playlist-pos") {
            Ok(current_id) => self.run_command(MpvCommand::PlaylistMove {
                from: id,
                to: current_id + 1,
            }),
            Err(msg) => Err(msg),
        }
    }

    pub fn playlist_remove_id(&self, id: usize) -> Result<(), Error> {
        self.run_command(MpvCommand::PlaylistRemove(id))
    }

    pub fn playlist_shuffle(&self) -> Result<(), Error> {
        self.run_command(MpvCommand::PlaylistShuffle)
    }

    pub fn seek(&self, seconds: f64, option: SeekOptions) -> Result<(), Error> {
        self.run_command(MpvCommand::Seek { seconds, option })
    }

    pub fn set_loop_file(&self, option: Switch) -> Result<(), Error> {
        let mut enabled = false;
        match option {
            Switch::On => enabled = true,
            Switch::Off => {}
            Switch::Toggle => match get_mpv_property_string(self, "loop-file") {
                Ok(value) => match value.as_ref() {
                    "false" => {
                        enabled = true;
                    }
                    _ => {
                        enabled = false;
                    }
                },
                Err(msg) => return Err(msg),
            },
        }
        set_mpv_property(self, "loop-file", json!(enabled))
    }

    pub fn set_loop_playlist(&self, option: Switch) -> Result<(), Error> {
        let mut enabled = false;
        match option {
            Switch::On => enabled = true,
            Switch::Off => {}
            Switch::Toggle => match get_mpv_property_string(self, "loop-playlist") {
                Ok(value) => match value.as_ref() {
                    "false" => {
                        enabled = true;
                    }
                    _ => {
                        enabled = false;
                    }
                },
                Err(msg) => return Err(msg),
            },
        }
        set_mpv_property(self, "loop-playlist", json!(enabled))
    }

    pub fn set_mute(&self, option: Switch) -> Result<(), Error> {
        let mut enabled = false;
        match option {
            Switch::On => enabled = true,
            Switch::Off => {}
            Switch::Toggle => match get_mpv_property::<bool>(self, "mute") {
                Ok(value) => {
                    enabled = !value;
                }
                Err(msg) => return Err(msg),
            },
        }
        set_mpv_property(self, "mute", json!(enabled))
    }

    /// # Description
    ///
    /// Sets the mpv property _<property>_ to _<value>_.
    ///
    /// ## Supported types
    /// - String
    /// - bool
    /// - f64
    /// - usize
    ///
    /// ## Input arguments
    ///
    /// - **property** defines the mpv property that should be retrieved
    /// - **value** defines the value of the given mpv property _<property>_
    ///
    /// # Example
    /// ```
    /// use mpvipc::{Mpv, Error};
    /// fn main() -> Result<(), Error> {
    ///     let mpv = Mpv::connect("/tmp/mpvsocket")?;
    ///     mpv.set_property("pause", true)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn set_property<T: SetPropertyTypeHandler<T>>(
        &self,
        property: &str,
        value: T,
    ) -> Result<(), Error> {
        T::set_property_generic(self, property, value)
    }

    pub fn set_speed(&self, input_speed: f64, option: NumberChangeOptions) -> Result<(), Error> {
        match get_mpv_property::<f64>(self, "speed") {
            Ok(speed) => match option {
                NumberChangeOptions::Increase => {
                    set_mpv_property(self, "speed", json!(speed + input_speed))
                }

                NumberChangeOptions::Decrease => {
                    set_mpv_property(self, "speed", json!(speed - input_speed))
                }

                NumberChangeOptions::Absolute => {
                    set_mpv_property(self, "speed", json!(input_speed))
                }
            },
            Err(msg) => Err(msg),
        }
    }

    pub fn set_volume(&self, input_volume: f64, option: NumberChangeOptions) -> Result<(), Error> {
        match get_mpv_property::<f64>(self, "volume") {
            Ok(volume) => match option {
                NumberChangeOptions::Increase => {
                    set_mpv_property(self, "volume", json!(volume + input_volume))
                }

                NumberChangeOptions::Decrease => {
                    set_mpv_property(self, "volume", json!(volume - input_volume))
                }

                NumberChangeOptions::Absolute => {
                    set_mpv_property(self, "volume", json!(input_volume))
                }
            },
            Err(msg) => Err(msg),
        }
    }

    pub fn stop(&self) -> Result<(), Error> {
        self.run_command(MpvCommand::Stop)
    }

    pub fn toggle(&self) -> Result<(), Error> {
        run_mpv_command(self, "cycle", &["pause"])
    }
}
