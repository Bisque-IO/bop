//! Command builder for spawning child processes.

use std::ffi::{OsStr, OsString};
use std::io;
use std::path::Path;
use std::process::Stdio;

use super::child::{Child, ExitStatus, Output};

/// A builder for spawning child processes.
///
/// This struct mirrors the API of [`std::process::Command`] but provides
/// async methods for spawning and managing child processes.
///
/// # Examples
///
/// Basic usage:
///
/// ```no_run
/// use maniac::process::Command;
///
/// # async fn example() -> std::io::Result<()> {
/// let mut child = Command::new("ls")
///     .arg("-la")
///     .spawn()?;
///
/// let status = child.wait().await?;
/// println!("Process exited with: {:?}", status);
/// # Ok(())
/// # }
/// ```
pub struct Command {
    inner: std::process::Command,
}

impl Command {
    /// Creates a new `Command` for launching the program at `program`.
    ///
    /// The default configuration is:
    /// - No arguments
    /// - Inherit the current process's environment
    /// - Inherit the current process's working directory
    /// - Inherit stdin/stdout/stderr
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use maniac::process::Command;
    ///
    /// let cmd = Command::new("ls");
    /// ```
    pub fn new<S: AsRef<OsStr>>(program: S) -> Self {
        Self {
            inner: std::process::Command::new(program),
        }
    }

    /// Adds an argument to pass to the program.
    ///
    /// Only one argument can be passed per use. So instead of:
    ///
    /// ```no_run
    /// # use maniac::process::Command;
    /// Command::new("sh")
    ///     .arg("-C /path/to/repo");
    /// ```
    ///
    /// usage would be:
    ///
    /// ```no_run
    /// # use maniac::process::Command;
    /// Command::new("sh")
    ///     .arg("-C")
    ///     .arg("/path/to/repo");
    /// ```
    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Self {
        self.inner.arg(arg);
        self
    }

    /// Adds multiple arguments to pass to the program.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use maniac::process::Command;
    ///
    /// Command::new("ls")
    ///     .args(["-l", "-a"]);
    /// ```
    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.inner.args(args);
        self
    }

    /// Sets the working directory for the child process.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use maniac::process::Command;
    ///
    /// Command::new("ls")
    ///     .current_dir("/tmp");
    /// ```
    pub fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Self {
        self.inner.current_dir(dir);
        self
    }

    /// Sets an environment variable for the child process.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use maniac::process::Command;
    ///
    /// Command::new("printenv")
    ///     .env("MY_VAR", "my_value");
    /// ```
    pub fn env<K, V>(&mut self, key: K, val: V) -> &mut Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.inner.env(key, val);
        self
    }

    /// Sets multiple environment variables for the child process.
    pub fn envs<I, K, V>(&mut self, vars: I) -> &mut Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.inner.envs(vars);
        self
    }

    /// Removes an environment variable from the child process.
    pub fn env_remove<K: AsRef<OsStr>>(&mut self, key: K) -> &mut Self {
        self.inner.env_remove(key);
        self
    }

    /// Clears all environment variables for the child process.
    pub fn env_clear(&mut self) -> &mut Self {
        self.inner.env_clear();
        self
    }

    /// Sets the stdin handle for the child process.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use maniac::process::{Command, Stdio};
    ///
    /// Command::new("cat")
    ///     .stdin(Stdio::piped());
    /// ```
    pub fn stdin<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.inner.stdin(cfg);
        self
    }

    /// Sets the stdout handle for the child process.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use maniac::process::{Command, Stdio};
    ///
    /// Command::new("echo")
    ///     .arg("hello")
    ///     .stdout(Stdio::piped());
    /// ```
    pub fn stdout<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.inner.stdout(cfg);
        self
    }

    /// Sets the stderr handle for the child process.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use maniac::process::{Command, Stdio};
    ///
    /// Command::new("ls")
    ///     .arg("/nonexistent")
    ///     .stderr(Stdio::piped());
    /// ```
    pub fn stderr<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.inner.stderr(cfg);
        self
    }

    /// Spawns the command as a child process, returning a handle to it.
    ///
    /// By default, stdin, stdout and stderr are inherited from the parent.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use maniac::process::Command;
    ///
    /// # fn main() -> std::io::Result<()> {
    /// let child = Command::new("ls").spawn()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn spawn(&mut self) -> io::Result<Child> {
        let child = self.inner.spawn()?;
        Child::from_std(child)
    }

    /// Executes the command and waits for it to finish, returning its exit status.
    ///
    /// This is a convenience method that spawns the child and immediately waits for it.
    /// Stdin, stdout, and stderr are inherited from the parent by default.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use maniac::process::Command;
    ///
    /// # async fn example() -> std::io::Result<()> {
    /// let status = Command::new("ls").status().await?;
    /// println!("Exited with: {:?}", status);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn status(&mut self) -> io::Result<ExitStatus> {
        self.spawn()?.wait().await
    }

    /// Executes the command and collects all of its output.
    ///
    /// This method sets up piped stdout and stderr, spawns the command,
    /// waits for it to finish, and collects all output.
    ///
    /// Stdin is set to null by default.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use maniac::process::Command;
    ///
    /// # async fn example() -> std::io::Result<()> {
    /// let output = Command::new("echo")
    ///     .arg("hello")
    ///     .output()
    ///     .await?;
    ///
    /// println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    /// # Ok(())
    /// # }
    /// ```
    pub async fn output(&mut self) -> io::Result<Output> {
        self.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = self.spawn()?;
        child.wait_with_output().await
    }

    /// Returns the path to the program that was given to [`Command::new`].
    pub fn get_program(&self) -> &OsStr {
        self.inner.get_program()
    }

    /// Returns an iterator of the arguments that will be passed to the program.
    pub fn get_args(&self) -> std::process::CommandArgs<'_> {
        self.inner.get_args()
    }

    /// Returns an iterator of the environment variables that will be set when the process is spawned.
    pub fn get_envs(&self) -> std::process::CommandEnvs<'_> {
        self.inner.get_envs()
    }

    /// Returns the working directory for the child process.
    pub fn get_current_dir(&self) -> Option<&Path> {
        self.inner.get_current_dir()
    }
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}

impl From<std::process::Command> for Command {
    fn from(cmd: std::process::Command) -> Self {
        Self { inner: cmd }
    }
}

