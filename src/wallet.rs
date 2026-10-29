//! The GitHub token in KDE Wallet, through kwalletd's own D-Bus
//! interface (`org.kde.KWallet` on `org.kde.kwalletd6`): the network
//! wallet, a `Libri` folder, one password entry. The token never goes
//! into a plain file or into `library.json`.
//!
//! Not the Secret Service API: on this computer `org.freedesktop.secrets`
//! is owned by gnome-keyring, and ksecretd only offers it under
//! `org.kde.secretservicecompat`, so a Secret Service client would put
//! the token in the wrong keyring. Every call here blocks; the app runs
//! them on tokio's blocking pool from an iced Task.

use zbus::blocking::{Connection, Proxy};

const SERVICE: &str = "org.kde.kwalletd6";
const PATH: &str = "/modules/kwalletd6";
const INTERFACE: &str = "org.kde.KWallet";
/// How Libri names itself to the wallet (the prompt says who asks).
const APP: &str = "Libri";
const FOLDER: &str = "Libri";
const KEY: &str = "github-token";

/// An open wallet, closed when dropped.
struct Wallet {
    proxy: Proxy<'static>,
    handle: i32,
}

type Outcome<T> = Result<T, String>;

fn open() -> Outcome<Wallet> {
    let connection = Connection::session().map_err(|e| format!("no session bus: {e}"))?;
    let proxy = Proxy::new_owned(connection, SERVICE, PATH, INTERFACE)
        .map_err(|e| format!("no KDE Wallet service: {e}"))?;
    let wallet: String = proxy
        .call("networkWallet", &())
        .map_err(|e| format!("KDE Wallet did not answer: {e}"))?;
    // The window id is 0: the wallet's own prompt is not attached to
    // Libri's window.
    let handle: i32 = proxy
        .call("open", &(wallet.as_str(), 0i64, APP))
        .map_err(|e| format!("KDE Wallet could not be opened: {e}"))?;
    if handle < 0 {
        return Err("KDE Wallet stayed closed".into());
    }
    Ok(Wallet { proxy, handle })
}

impl Wallet {
    fn call<R>(
        &self,
        method: &str,
        body: &(impl serde::Serialize + zbus::zvariant::DynamicType),
    ) -> Outcome<R>
    where
        R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
    {
        self.proxy
            .call(method, body)
            .map_err(|e| format!("KDE Wallet {method} failed: {e}"))
    }

    fn has_entry(&self) -> Outcome<bool> {
        self.call("hasEntry", &(self.handle, FOLDER, KEY, APP))
    }
}

impl Drop for Wallet {
    fn drop(&mut self) {
        let _: Result<i32, _> = self.proxy.call("close", &(self.handle, false, APP));
    }
}

/// The stored token, or `None` when there is none. An error means the
/// wallet could not be read; the app then treats itself as disconnected
/// and asks for the token again.
pub fn read_token() -> Outcome<Option<String>> {
    let wallet = open()?;
    if !wallet.has_entry()? {
        return Ok(None);
    }
    let token: String = wallet.call("readPassword", &(wallet.handle, FOLDER, KEY, APP))?;
    Ok(Some(token).filter(|t| !t.is_empty()))
}

pub fn write_token(token: &str) -> Outcome<()> {
    let wallet = open()?;
    let has_folder: bool = wallet.call("hasFolder", &(wallet.handle, FOLDER, APP))?;
    if !has_folder {
        let created: bool = wallet.call("createFolder", &(wallet.handle, FOLDER, APP))?;
        if !created {
            return Err("KDE Wallet refused the Libri folder".into());
        }
    }
    let result: i32 = wallet.call("writePassword", &(wallet.handle, FOLDER, KEY, token, APP))?;
    if result != 0 {
        return Err(format!("KDE Wallet refused the token ({result})"));
    }
    Ok(())
}

pub fn delete_token() -> Outcome<()> {
    let wallet = open()?;
    if !wallet.has_entry()? {
        return Ok(());
    }
    let result: i32 = wallet.call("removeEntry", &(wallet.handle, FOLDER, KEY, APP))?;
    if result != 0 {
        return Err(format!("KDE Wallet kept the token ({result})"));
    }
    Ok(())
}
