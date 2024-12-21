use json;

fn main() {

    let socket_path = "/var/run/docker.sock";
    let mut stream = 
        std::os::unix::net::UnixStream::connect(socket_path)
        .unwrap();
    let request = "GET /containers/json HTTP/1.1\r\n\
                   Host: localhost\r\n\
                   Connection: close\r\n\
                   \r\n";
    std::io::Write::write_all(&mut stream, request.as_bytes())
        .unwrap();

    let mut response = String::new();
    std::io::Read::read_to_string(&mut stream, &mut response)
        .unwrap();
    
    println!("Response\n{}", response);

    let mut lines = response.lines();
    let body = lines.find(|l| l.starts_with('['))
        .expect("Response body not found!");
    let json_body = json::parse(body)
        .expect("Failed to parse response body!");
    
    let members = json_body.members();
    let container_names: Vec<&str> = members.map(
        |ref m| m["Names"][0].as_str().unwrap()
    ).collect();
    
    //println!("container names: {:?}", container_names);

    let mut log_json = json::JsonValue::new_object();
    for container_name in container_names {
        let mut stats_stream = 
            std::os::unix::net::UnixStream::connect(socket_path)
            .unwrap();
        let stats_request = format!(
            "GET /containers{}/stats?stream=false&one-shot=true HTTP/1.1\r\n\
             Host: localhost\r\n\
             Connection: close\r\n\r\n",
             container_name
        );
        //println!("Request:\n{}", stats_request);
        std::io::Write::write_all(
            &mut stats_stream, stats_request.as_bytes()
        ).unwrap();
        let mut stats_response = String::new();
        std::io::Read::read_to_string(
            &mut stats_stream, &mut stats_response
        ).unwrap();
        println!("Response:\n{}", stats_response);

        let stats_data = stats_response.lines()
            .find(|l| l.starts_with('{'))
            .expect("cannot find response json body");
        let stats = json::parse(stats_data)
            .expect("cannot parse response json body");
        let total = 
            stats["cpu_stats"]["cpu_usage"]["total_usage"]
            .as_u64()
            .expect("cannot get total usage as u64");
        let system =
            stats["cpu_stats"]["system_cpu_usage"]
            .as_u64()
            .expect("cannot get kernelmode usage as u64");
        let number_cpus =
            stats["cpu_stats"]["online_cpus"]
            .as_u32()
            .expect("cannot get online cpus as u64");
        let used_memory =
            stats["memory_stats"]["usage"]
            .as_u64()
            .expect("cannot get memory usage as u64")
            -
            stats["memory_stats"]["stats"]["cache"]
            .as_u64()
            .expect("cannot get cached memory as u64");
        let available_memory =
            stats["memory_stats"]["limit"]
            .as_u64()
            .expect("cannot get memory limit as u64");
        let net_rx =
            stats["networks"]["eth0"]["rx_bytes"]
            .as_u64()
            .expect("cannot get net eth0 rx_bytes as u64");
        let net_tx =
            stats["networks"]["eth0"]["tx_bytes"]
            .as_u64()
            .expect("cannot get net eth0 tx_bytes as u64");
        let blkio_read =
            stats["blkio_stats"]["io_service_bytes_recursive"]
            .members()
            .find(|ref m| m["op"] == "read")
            .and_then(|v| v.as_u64());
        let blkio_write =
            stats["blkio_stats"]["io_service_bytes_recursive"]
            .members()
            .find(|ref m| m["op"] == "write")
            .and_then(|v| v.as_u64());
            
        // in one-shot mode, pre-stats are not available.
        // we have to take diff by ourselves

        //println!(
        //    "Used memory: {:.2}%", 
        //    used_memory as f32 / available_memory as f32 * 100.0_f32
        //);

        log_json[container_name.replace("/","")] = json::object!{
            cpu: { 
                total: total, 
                system: system, 
                ncpu: number_cpus
            },
            memory: {
                used: used_memory, 
                available: available_memory
            },
            io: {
                read: blkio_read,
                write: blkio_write, 
            },
            net: {
                send: net_tx,
                recv: net_rx,
            }
        };
        //println!("{}", log_json.dump());
    }
    println!("{}", log_json.dump());
}

