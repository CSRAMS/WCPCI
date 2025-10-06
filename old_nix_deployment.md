# Deploying

## Step 1. Provision secrets in `./secrets`:

- `saml_cert.pem`
- `saml_key.pem`
- `tls_cert.pem`
- `tls_key.pem`
- Fill in `.env` with client secrets and admin usernames
- Generate a secret key for `.env` with `openssl rand -base64 32`

To generate self-signed certificate for SAML do:
```
openssl req -newkey rsa:4096 -x509 -sha512 -days 3650 -nodes -out secrets/saml_cert.pem -keyout secrets/saml_key.pem
```

## Step 2. Edit `rocket_config.nix`

This file is used to generate a `rocket.toml` file that the server uses as it's primary configuration. The options should be fairly self-documenting, but there are some docs [in the deployment guide](https://github.com/Bwc9876/WCPCI/blob/dev/DEPLOYMENT.md).

> **Warning** <br>
Don't put secrets in Nix code! They will be world-readable! Deploy secrets through some other secure channel.

## Step 3. Build and load the docker image

You can either build the container image, save it to disk, then import it into Docker, or stream it into Docker as it's generated. Streaming saves on disk space.

```sh
nix run .#container-stream | sudo docker load

# OR

nix build .#container
# Copy `./result` to remote machine if necessary (don't forget `secrets/`)
sudo docker load -i result
```

## Step 4. Run the container

To run, the container needs:
- Secrets on `/secrets`
- Port 443 (unless you've changed it)
- Persistent volume on `/database` (not strictly needed, but you probably want it)

For example:
```sh
sudo docker run --rm -d -v /path/to/secrets:/secrets:ro -v wcpc_database:/database -p 443:443/tcp wcpc
```

> **Note** <br>
`--rm` will remove the container when it stops. All data is saved in the database, which should be persisted. If you are using a named volume or bind mount, it will persist. Otherwise, don't specify `--rm` if you want to keep your data.

# Updating

1. Run `nix flake update`
2. Rebuild the image and load it into docker
3. Make a backup of the database.
   - If you have a volume named `wcpc_database`, run:
     ```sh
     docker run --rm -v wcpc_database:/database -v $(pwd):/backup busybox cp  database/database.sqlite /backup/database-backup-$(date -I).sqlite
     ```
4. Restart the container
5. (Optional) Run `docker image prune -a` to remove unused images. Don't run if you don't want to remove unused images.
