# Environnement de développement : Windows + WSL2

Machine de référence : 32 Go de RAM, distribution WSL installée sur `D:`.
Tout le travail (code, PX4, build Yocto) se fait **dans le système de fichiers
Linux** de la distribution. Travailler depuis `/mnt/c` ou `/mnt/d` divise les
performances d'I/O par 10 ou plus, ce qui rend un build Yocto inutilisable.

## 1. Installer Ubuntu 24.04 sur D:

PowerShell (administrateur) :

```powershell
wsl --update
mkdir D:\WSL
wsl --install -d Ubuntu-24.04 --location D:\WSL\Ubuntu-24.04
```

Si la distribution est déjà installée sur `C:`, la déplacer :

```powershell
wsl --shutdown
wsl --export Ubuntu-24.04 D:\WSL\ubuntu-24.04.tar
wsl --unregister Ubuntu-24.04
wsl --import Ubuntu-24.04 D:\WSL\Ubuntu-24.04 D:\WSL\ubuntu-24.04.tar --version 2
```

(Après un `--import`, l'utilisateur par défaut est `root` : ajouter
`[user]\ndefault=<ton_user>` dans `/etc/wsl.conf`.)

## 2. Ressources : `%UserProfile%\.wslconfig`

```ini
[wsl2]
memory=24GB          # laisse 8 Go à Windows
swap=16GB
swapFile=D:\\WSL\\swap.vhdx
# processors=        # par défaut : tous les cœurs logiques, ce qu'on veut

[experimental]
sparseVhd=true            # le VHDX rend l'espace libéré à D:
autoMemoryReclaim=gradual # rend la RAM du cache à Windows après un build
```

Puis `wsl --shutdown` et relancer la distribution. Vérifier avec `free -h` et `nproc`.

Conseil Windows Defender : exclure `D:\WSL` de l'analyse en temps réel,
sinon chaque écriture dans le VHDX est inspectée.

## 3. Rust

```bash
sudo apt update && sudo apt install -y build-essential pkg-config git curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup component add clippy rustfmt
rustup target add thumbv7em-none-eabihf aarch64-unknown-linux-gnu
```

## 4. PX4 SITL + Gazebo

```bash
cd ~
git clone https://github.com/PX4/PX4-Autopilot.git --recursive -b v1.17.0
bash ./PX4-Autopilot/Tools/setup/ubuntu.sh
# fermer et rouvrir le terminal, puis :
cd ~/PX4-Autopilot && make px4_sitl gz_x500
```

Gazebo s'affiche via WSLg (Windows 11). Si la fenêtre ne s'ouvre pas ou si le
rendu est trop lent, lancer sans interface : `HEADLESS=1 make px4_sitl gz_x500`.
Vigie n'a pas besoin de la fenêtre Gazebo, seulement du flux MAVLink.

### QGroundControl côté Windows

Installer QGroundControl pour Windows, puis :

1. dans WSL : `ip addr show eth0 | grep inet` → noter l'IP ;
2. QGC → *Application Settings* → *Comm Links* → *Add* : UDP, port `18570`,
   serveur `<IP WSL>:18570`.

L'IP de WSL change à chaque redémarrage. Avec Windows 11 22H2+, on peut
activer `networkingMode=mirrored` dans `.wslconfig` pour que WSL partage
l'IP de Windows et que QGC se connecte tout seul sur `localhost`.

## 5. Lancer Vigie contre le SITL

Dans un second terminal WSL :

```bash
cd ~/vigie
RUST_LOG=info,vigied=debug cargo run -p vigied -- --config config/vigie.toml
```

`vigied` écoute le flux « onboard » de PX4 sur UDP 14540. Au démarrage il doit
afficher `autopilote détecté`, puis un résumé d'état toutes les 5 s.

Sans QGroundControl, PX4 v1.17 refuse d'armer (`Preflight Fail: No connection to
the GCS`). Pour voler sans station sol, dans la console `pxh>` :

```
param set NAV_DLL_ACT 0
```

Le paramètre est conservé entre deux lancements du SITL. Vigie, lui, déclenche
toujours un RTL après 5 s sans heartbeat de station sol : c'est le comportement voulu.

## 6. Yocto (semaine 3)

Prérequis hôte pour Yocto 6.0 « Wrynose » :

```bash
sudo apt install -y gawk wget git diffstat unzip texinfo gcc build-essential \
  chrpath socat cpio python3 python3-pip python3-pexpect xz-utils debianutils \
  iputils-ping python3-git python3-jinja2 python3-subunit zstd liblz4-tool \
  file locales libacl1
sudo locale-gen en_US.UTF-8
```

Réglages `local.conf` adaptés à 24 Go de RAM dans WSL (la compilation de
`rust-native` et de LLVM est gourmande, compter ~2 Go par job) :

```
BB_NUMBER_THREADS = "6"
PARALLEL_MAKE = "-j 8"
DL_DIR = "${HOME}/yocto-cache/downloads"
SSTATE_DIR = "${HOME}/yocto-cache/sstate"
INHERIT += "rm_work"
```

`rm_work` supprime les répertoires de travail après chaque recette : le
premier build complet passe d'environ 120 Go à 40 Go. Avec 500 Go libres sur
`D:`, c'est confortable.
