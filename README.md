# ko-calendar

Calendario con Google para escritorios Linux y Windows.

Consulta tus eventos y recibe recordatorios desde la bandeja del sistema. Las versiones oficiales incluyen la conexión con Google: no necesitas compilar ni crear credenciales propias.

## Windows · Scoop

Abre PowerShell e instala la aplicación:

```powershell
scoop install git
scoop bucket add ko https://github.com/Anemone3/ko-calendar
scoop install ko/ko-calendar
ko-calendar
```

El bucket se añade una sola vez. Si aún no tienes Scoop, sigue su [guía de instalación](https://github.com/ScoopInstaller/Install#readme) desde una ventana de PowerShell sin permisos de administrador.

Para actualizar, cierra la aplicación desde su icono de bandeja y ejecuta:

```powershell
scoop update
scoop update ko-calendar
```

Para desinstalar:

```powershell
scoop uninstall ko-calendar
```

La desinstalación retira los accesos directos y conserva tus datos locales.

## Arch Linux · yay

Necesitas `git`, `base-devel` y [yay](https://github.com/Jguer/yay#installation). Instala desde el PKGBUILD de este repositorio:

```sh
sudo pacman -S --needed git base-devel
git clone https://github.com/Anemone3/ko-calendar.git
yay -Bi ./ko-calendar/packaging/aur/ko-calendar-bin
ko-calendar
```

Este paquete descarga el binario oficial; no compila la aplicación en Rust. Ejecuta `yay` como usuario normal.

El paquete todavía no está publicado en AUR, por lo que `yay -S ko-calendar` y `yay -S ko-calendar-bin` no están disponibles.

Para actualizar desde la carpeta donde clonaste el repositorio:

```sh
git -C ko-calendar pull --ff-only
yay -Bi ./ko-calendar/packaging/aur/ko-calendar-bin
```

Para desinstalar:

```sh
sudo pacman -Rns ko-calendar-bin
```

### Sin yay

Puedes usar el mismo PKGBUILD con `makepkg`:

```sh
sudo pacman -S --needed git base-devel
git clone https://github.com/Anemone3/ko-calendar.git
cd ko-calendar/packaging/aur/ko-calendar-bin
makepkg -si
```

En Linux necesitas una bandeja de sistema para acceder al menú, un servicio de notificaciones y un almacén compatible con Secret Service, como GNOME Keyring o KWallet, para guardar la sesión de Google.

## Descarga directa

Los paquetes oficiales son para **x86_64 / x64** y están en [Releases](https://github.com/Anemone3/ko-calendar/releases/latest):

| Sistema | Archivo | Instalación |
| --- | --- | --- |
| Windows | `ko-calendar-windows-x64.zip` | Extrae el ZIP y abre `ko-calendar.exe`. |
| Arch Linux | `ko-calendar-bin-VERSION-1-x86_64.pkg.tar.zst` | Instala el archivo descargado con `sudo pacman -U nombre-del-archivo.pkg.tar.zst`. |
| Linux | `ko-calendar-linux-x86_64.tar.gz` | Extrae el archivo y ejecuta `./ko-calendar` dentro de la carpeta extraída. |

El archivo de Linux requiere las bibliotecas y servicios de tu escritorio. En Arch, el paquete de yay o pacman instala las dependencias declaradas.

## Conectar Google Calendar

1. Abre **Configuración** desde el icono de bandeja.
2. Pulsa **Google Sync**, selecciona tu cuenta y concede los permisos de lectura.
3. Vuelve a la aplicación para consultar el calendario principal y recibir los avisos.

La conexión con Google está en producción. Mientras se completa su verificación, Google puede mostrar una advertencia de aplicación sin verificar y aplicar su límite de usuarios.

Para retirar la autorización y borrar la copia activa del calendario sincronizado, desconecta Google desde la aplicación. También puedes gestionar el acceso en [tu cuenta de Google](https://myaccount.google.com/connections).

[Política de privacidad](https://anemone3.github.io/ko-calendar/privacy.html) · [Reportar un problema](https://github.com/Anemone3/ko-calendar/issues) · [Licencia MIT](LICENSE)
