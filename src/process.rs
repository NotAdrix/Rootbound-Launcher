//! Seguimiento de los procesos de un juego a partir de su carpeta de instalación.

use crate::legendary::hidden_command;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread::sleep;
use std::time::{Duration, Instant};
use std::os::windows::io::AsRawHandle;
use sysinfo::{Pid, ProcessRefreshKind, System, UpdateKind};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Threading::{CreateMutexW, OpenMutexW};

const START_TIMEOUT: Duration = Duration::from_secs(90);
const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// Lecturas seguidas sin procesos antes de darlo por cerrado: los juegos encadenan
/// bootstrapper, launcher y cliente, y entre uno y otro puede no haber ninguno.
const MISSES_TO_EXIT: u32 = 3;

/// Si otro hilo falló con el candado tomado, el dato sigue siendo útil: se recupera
/// en vez de propagar el fallo a todo el launcher.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Juegos que el usuario acaba de cerrar: su vigilante no tiene que esperar más.
static CLOSED_BY_USER: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

fn take_closed_by_user(root: &Path) -> bool {
    let root = normalize(root);
    let mut closed = lock(&CLOSED_BY_USER);
    let was_closed = closed.contains(&root);
    closed.retain(|path| *path != root);
    was_closed
}

fn normalize(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().to_lowercase().replace('/', "\\"))
}

/// Una ruta vacía o la raíz de una unidad coincidiría con procesos ajenos.
fn is_safe_root(root: &Path) -> bool {
    root.is_absolute() && root.parent().is_some()
}

pub fn is_inside(exe: &Path, root: &Path) -> bool {
    normalize(exe).starts_with(normalize(root))
}

fn refresh(system: &mut System) {
    system.refresh_processes_specifics(
        ProcessRefreshKind::new().with_exe(UpdateKind::OnlyIfNotSet),
    );
}

fn listed_inside(system: &System, root: &Path) -> Vec<Pid> {
    if !is_safe_root(root) {
        return Vec::new();
    }
    let own_pid = sysinfo::get_current_pid().ok();
    system
        .processes()
        .iter()
        .filter(|(pid, process)| {
            Some(**pid) != own_pid && process.exe().is_some_and(|exe| is_inside(exe, root))
        })
        .map(|(pid, _)| *pid)
        .collect()
}

fn processes_inside(system: &mut System, root: &Path) -> Vec<Pid> {
    refresh(system);
    listed_inside(system, root)
}

/// Fotografía de los procesos para hacer varias comprobaciones seguidas.
pub struct ProcessSnapshot(System);

impl ProcessSnapshot {
    pub fn new() -> Self {
        Self(System::new())
    }

    pub fn refresh(&mut self) {
        refresh(&mut self.0);
    }

    pub fn folder_is_running(&self, root: &Path) -> bool {
        !listed_inside(&self.0, root).is_empty()
    }

    pub fn exe_is_running(&self, exe: &Path) -> bool {
        let wanted = normalize(exe);
        self.0
            .processes()
            .values()
            .any(|process| process.exe().is_some_and(|path| normalize(path) == wanted))
    }
}

/// Espera a que el juego termine. Devuelve `false` si nunca llegó a arrancar.
pub fn wait_for_exit(root: &Path) -> bool {
    take_closed_by_user(root);
    let mut system = System::new();
    let launched_at = Instant::now();
    let mut seen = false;
    let mut misses = 0;
    loop {
        sleep(POLL_INTERVAL);
        if take_closed_by_user(root) {
            return true;
        }
        if !processes_inside(&mut system, root).is_empty() {
            seen = true;
            misses = 0;
        } else if seen {
            misses += 1;
            if misses >= MISSES_TO_EXIT {
                return true;
            }
        } else if launched_at.elapsed() > START_TIMEOUT {
            return false;
        }
    }
}

pub fn kill_tree(pid: u32) {
    let _ = hidden_command("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .output();
}

pub fn kill_game(root: &Path) {
    let mut system = System::new();
    for _ in 0..10 {
        let pids = processes_inside(&mut system, root);
        if pids.is_empty() {
            break;
        }
        for pid in pids {
            kill_tree(pid.as_u32());
        }
        sleep(Duration::from_millis(300));
    }
    lock(&CLOSED_BY_USER).push(normalize(root));
}

/// Carpetas que nunca se pueden borrar, ni ellas ni ninguna que las contenga.
fn holds_protected_folder(root: &Path) -> bool {
    let from_env = ["ProgramFiles", "ProgramFiles(x86)", "ProgramData", "SystemRoot"]
        .iter()
        .filter_map(|name| std::env::var_os(name).map(PathBuf::from));
    from_env.chain(dirs::home_dir()).any(|folder| is_inside(&folder, root))
}

/// Borra la carpeta de un juego. Devuelve `false` si no pudo o no era seguro hacerlo.
pub fn remove_leftover_folder(root: &Path) -> bool {
    if !root.exists() {
        return true;
    }
    is_safe_root(root) && !holds_protected_folder(root) && std::fs::remove_dir_all(root).is_ok()
}

/// Cierra los procesos de ese ejecutable, salvo este mismo.
pub fn kill_exe(path: &Path) {
    let mut system = System::new();
    refresh(&mut system);
    let target = normalize(path);
    let own_pid = sysinfo::get_current_pid().ok();
    for (pid, process) in system.processes() {
        let same_exe = process.exe().is_some_and(|exe| normalize(exe) == target);
        if same_exe && Some(*pid) != own_pid {
            kill_tree(pid.as_u32());
        }
    }
}

pub fn is_elevated() -> bool {
    // SAFETY: función sin argumentos de shell32.
    unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
}

/// Abre un programa como lo haría el Explorador. Con `elevated`, Windows pide permiso
/// de administrador; también lo pide solo si el programa lo exige.
pub fn shell_open(program: &Path, arguments: &str, elevated: bool) -> bool {
    let verb = wide(if elevated { "runas" } else { "open" });
    let file = wide(&program.display().to_string());
    let params = wide(arguments);
    // SAFETY: todas las cadenas terminan en cero y viven durante la llamada.
    let result = unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            std::ptr::null(),
            windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW devuelve un valor mayor que 32 si funcionó.
    result as usize > 32
}

/// ¿Se puede escribir en `dir` sin ser administrador? Se prueba en la carpeta o, si no
/// existe aún, en el primer antecesor que sí exista.
pub fn can_write(dir: &Path) -> bool {
    let mut existing = dir;
    while !existing.exists() {
        match existing.parent() {
            Some(parent) => existing = parent,
            None => return false,
        }
    }
    let probe = existing.join(format!("rb_write_test_{}.tmp", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Las operaciones sobre `dir` necesitan administrador: ni lo somos ni se puede escribir.
pub fn needs_elevation(dir: &Path) -> bool {
    !is_elevated() && !can_write(dir)
}

/// Lanza un programa con `runas` (pide administrador) y espera a que termine.
/// Devuelve su código de salida, o `None` si el usuario no concedió el permiso.
pub fn shell_execute_wait(file: &Path, parameters: &str) -> Option<u32> {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};

    let verb = wide("runas");
    let file_w = wide(&file.display().to_string());
    let params_w = wide(parameters);
    // SAFETY: SHELLEXECUTEINFOW admite ceros; los punteros viven durante la llamada.
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file_w.as_ptr();
    info.lpParameters = params_w.as_ptr();
    info.nShow = windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    // SAFETY: `info` está relleno; se comprueba el handle antes de usarlo.
    let launched = unsafe { ShellExecuteExW(&mut info) != 0 } && !info.hProcess.is_null();
    if !launched {
        return None;
    }
    // SAFETY: `hProcess` es válido hasta que lo cerramos aquí.
    let exit = unsafe {
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code: u32 = 1;
        if WaitForSingleObject(info.hProcess, 0) == WAIT_OBJECT_0 {
            GetExitCodeProcess(info.hProcess, &mut code);
        }
        CloseHandle(info.hProcess);
        code
    };
    Some(exit)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Mutex con nombre que indica si otro proceso del launcher está activo.
pub struct NamedLock(HANDLE);

impl NamedLock {
    /// `None` si ya lo tiene otro proceso.
    pub fn acquire(name: &str) -> Option<Self> {
        let name = wide(name);
        // SAFETY: `name` termina en cero y vive durante la llamada.
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        // SAFETY: se lee justo después de la llamada anterior.
        let already_existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        if handle.is_null() {
            return None;
        }
        let lock = Self(handle);
        (!already_existed).then_some(lock)
    }

    pub fn exists(name: &str) -> bool {
        const SYNCHRONIZE: u32 = 0x0010_0000;
        let name = wide(name);
        // SAFETY: `name` termina en cero y vive durante la llamada.
        let handle = unsafe { OpenMutexW(SYNCHRONIZE, 0, name.as_ptr()) };
        if handle.is_null() {
            return false;
        }
        // SAFETY: `handle` es válido y solo se cierra aquí.
        unsafe { CloseHandle(handle) };
        true
    }
}

impl Drop for NamedLock {
    fn drop(&mut self) {
        // SAFETY: el handle es propio y solo se cierra aquí.
        unsafe { CloseHandle(self.0) };
    }
}

/// Al morir el launcher, Windows cierra este job y mata lo que contenga. Solo deben
/// entrar descargas y actualizaciones: un juego abierto tiene que sobrevivir.
pub struct KillOnExitJob(HANDLE);

// SAFETY: un handle de job se puede usar desde cualquier hilo.
unsafe impl Send for KillOnExitJob {}
// SAFETY: la única operación compartida, `assign`, es segura entre hilos en Win32.
unsafe impl Sync for KillOnExitJob {}

impl KillOnExitJob {
    pub fn new() -> Option<Self> {
        // SAFETY: los punteros nulos piden atributos y nombre por defecto.
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return None;
        }
        // SAFETY: la estructura admite el patrón de ceros y se pasa con su tamaño real.
        let configured = unsafe {
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&info).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        (configured != 0).then_some(Self(job))
    }

    pub fn assign(&self, child: &std::process::Child) {
        // SAFETY: ambos handles son válidos mientras dura la llamada.
        unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle()) };
    }
}

impl Drop for KillOnExitJob {
    fn drop(&mut self) {
        // SAFETY: el handle es propio y solo se cierra aquí.
        unsafe { CloseHandle(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_inside_the_install_folder_ignoring_case_and_slashes() {
        let root = Path::new(r"G:\Fortnite");
        assert!(is_inside(Path::new("g:/fortnite/FortniteGame/x.exe"), root));
        assert!(!is_inside(Path::new(r"G:\FortniteOther\x.exe"), root));
    }

    #[test]
    fn only_processes_in_the_job_die_with_it() {
        let spawn = || hidden_command("ping").args(["-n", "30", "127.0.0.1"]).spawn().unwrap();
        let job = KillOnExitJob::new().expect("job");
        let mut download = spawn();
        let mut game = spawn();
        job.assign(&download);

        drop(job);
        sleep(Duration::from_secs(1));

        let download_died = download.try_wait().unwrap().is_some();
        let game_survived = game.try_wait().unwrap().is_none();
        let _ = game.kill();
        let _ = game.wait();
        let _ = download.kill();
        let _ = download.wait();
        assert!(download_died);
        assert!(game_survived);
    }

    #[test]
    fn a_named_lock_is_exclusive_while_held() {
        let name = format!("Local\\RootboundTest{}", std::process::id());
        assert!(!NamedLock::exists(&name));
        let lock = NamedLock::acquire(&name).expect("first");
        assert!(NamedLock::exists(&name));
        assert!(NamedLock::acquire(&name).is_none());
        drop(lock);
        assert!(!NamedLock::exists(&name));
    }

    #[test]
    fn leftovers_are_removed_but_never_a_folder_holding_the_user_profile() {
        let folder = std::env::temp_dir().join(format!("rootbound_left_{}", std::process::id()));
        std::fs::create_dir_all(folder.join("sub")).unwrap();
        std::fs::write(folder.join("sub").join("a.ini"), "x").unwrap();
        assert!(remove_leftover_folder(&folder));
        assert!(!folder.exists());

        let home = dirs::home_dir().unwrap();
        assert!(!remove_leftover_folder(home.parent().unwrap()));
        assert!(home.exists());
        let program_files = std::env::var_os("ProgramFiles").map(PathBuf::from).unwrap();
        assert!(!remove_leftover_folder(&program_files));
        assert!(program_files.exists());
    }

    #[test]
    fn rejects_roots_that_would_match_unrelated_processes() {
        assert!(!is_safe_root(Path::new("")));
        assert!(!is_safe_root(Path::new(r"C:\")));
        assert!(is_safe_root(Path::new(r"G:\Fortnite")));
    }
}
