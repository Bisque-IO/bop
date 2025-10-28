# Debian

## Create User

```bash
sudo adduser $USERNAME
sudo usermod -aG sudo $USERNAME
```

# FreeBSD

```bash
su -
```

or

```bash
sudo -i
```

```bash
adduser
pw groupmod wheel -m $USERNAME
visudo
$USERNAME ALL=(ALL) ALL
```
