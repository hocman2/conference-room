## Presentation
A Rust WebRTC SFU supported by the Mediasoup library. Probably not suited for large scale deployment. Has not been stress-tested nor extensivly tested.

## Instructions
Run the `sfu` binary to run the listening server (defaults on port 8000)

### Deploying on the public internet
First of all, if you intend to deploy the server for use on the public internet you must set some environment variables prior to that:
* `TLS_MODE=1` This is to tell the program that you intend to run it in secure mode. Modern browsers require the use of HTTPS to even use WebRTC in the first place
* `TLS_CERT_PATH=<some path>` The path of the HTTPS certificate
* `TLS_KEY_PATH=<some path>` The path of the HTTPS key
* `PUBLIC_IP=<the public IP of the machine running this program>` This is required for WebRTC communication between the server and clients. If not set, the SFU won't be able to listen or send streams.

You will need to redirect by the use of a proxy any connection at the desired path of your web server to `https://127.0.0.1:<8000 or the port you chose>/ws`. This is the entry point of the application to upgrade to websocket connection before being able to send a media.

For example:
`https://mydomain.com/api/conference --[Redirected to]--> https://127.0.0.1/ws` 

### Monitoring
The project features a second binary: "monitor"
This is intended to be a program that connects to a running SFU to retrieve data and easily diagnostic any issues.
The bare bone framework has been made SFU side for sending events and should work correctly (or with minimal tweaks) but there is still some work to be done to allow a client to retrieve global data without waiting for an event.
Nothing has been done on the client itself yet.

Monitors should connect to a running SFU through a secure connection and either query data of interest (only when needed to not overload the SFU with data sending) or responses to event of interest.
Data may include:
* SFU stats: number of active workers, number of total participants, number of consumers/worker, etc.
* Scoped stats: depends on what the client is listening for, list of rooms, room stats, room events, etc.

### Yet to do
* The whole monitoring thing
* Need to create new routers in new workers when the capacity is at max (as stated here: https://mediasoup.org/documentation/v3/scalability/)
* Some robust testing methodology
