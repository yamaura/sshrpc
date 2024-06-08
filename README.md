# sshrpc

This crate simplifies the process of automating tasks on remote machines using SSH (Secure Shell).
Normally, automating over SSH involves manually handling commands and capturing their standard output and standard error, which can be cumbersome.
This crate provides a more streamlined approach by enabling remote procedure calls (RPC) through an SSH port forwarding setup.

## Features

* Remote Procedure Calls: Utilize `tarpc` for RPC implementation, which allows for calling remote functions as if they were local.
* SSH Port Forwarding: Automatically set up SSH port forwarding to communicate with the remote RPC server, simplifying the connection setup.
* Serialization: Implements `tokio_serde` with `bincode` for efficient data serialization and transmission over the network.

## How It Works

The crate enables you to deploy a local RPC server program to a remote machine via SSH.
Once the server is deployed, SSH port forwarding is used to establish a communication channel between the local machine and the remote server.
This setup allows for easy execution of automation scripts that interact seamlessly with the remote environment.

## Example

You can see [examples](examples)
