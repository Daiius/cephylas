# Cephylas Docker Container Telemetrer
Simple server load logging system for Linux systems,
written in Rust.

Cephylas comes from "cephonodes hylas", a kind of moth with clear wings.

## Memo
```mermaid
classDiagram
    class Usages {
        time: String
        stats: Vec(HashMap(String,Usage))
    }
    class Usage {
        cpu: CpuUsage
        memory: MemoryUsage
        io: IoUsage
        net: NetUsage
    }

    class LogCache
    note for LogCache "HashMap(String,Usages)"

    class LogCacheUsage {
        cpu: Vec(CpuUsage)
        memory: Vec(MemoryUsage)
        io: Vec(IoUsage)
        net: Vec(NetUsage)
    }
```
```mermaid
sequenceDiagram
    participant docker as (Docker socket)
    participant stream as TcpStream
    participant buffer as buffer<br/>String
    participant container_names as container names<br/>String
    participant struct as Usages<br/>struct
    participant map as HashMap&lt;ContainerName,UsageData&gt;
    participant log as (log file)
    participant cache as LogCache<br/>a
    
    note over docker,cache : log cache initialization

    note over docker,cache : log record in every ticks

    docker -->> stream : /containers<br/>/stats
    stream -->> buffer : stream.read(&mut buffer)
    buffer -->> map : container_name
    buffer -->> map : UsageData


```

```mermaid
sequenceDiagram
    actor user
    participant cephylas
    box cephylas/* 
        participant watch
        participant log
        participant time

    end
    participant ./daily
    participant ./weekly

    user ->> cephylas : start with config
    cephylas ->> watch : watch(config: &Config)
    loop every daily unit time
        watch ->> log : log_daily()
        log ->> time : format_time(&time)<br/>using libc crate
        time -->> log : (time string)
        log ->> ./daily : json line log<br/>using json crate
        opt every weekly unit time
            watch ->> log : log_weekly()
            log ->> ./daily : read data to summarize
            ./daily -->> log : 
            log ->> ./weekly : write summarized data
        end
    end
```


```mermaid
sequenceDiagram
    actor user
    participant app as cephylas<br/>application
    participant worker as cephylas<br/>worker
    participant os as Linux
    participant database as database

    user ->> app    : run
    app  ->> worker : start
    activate worker
    loop Every second
        worker ->> os  : check <br/>cpu/network/disk 
        os -->> worker : /proc/stats <br/>/proc/net/dev
        opt detect change | active state | 10min passed
            worker ->> app   : log data with timestamp
            app ->> database : record log data to database
        end
    end
    user ->> app : terminate
    app  ->> worker : stop
    deactivate worker
```

## Log summarization plan
```mermaid
sequenceDiagram
    participant app as cephylas<br/>application
    participant daily
    participant daily.1
    participant weekly
    participant weekly.1
    participant monthly
    participant monthly.1

    app ->> daily : add log<br/>@daily unit time

    opt every weekly unit time
        daily ->> app: read latest<br/> ?? data length
        app ->> weekly : summarize and add
    end

    opt every monthly unit time
        weekly ->> app: read latest<br/> ?? data
        app ->> monthly: summarize and add
    end

    opt every 00:00
        daily -->> daily.1 : rename
        app ->> daily.1 : delete old daily log
    end

    opt every Monday 00:00
        weekly -->> weekly.1 : rename
    end

    opt every 1st
        monthly -->> monthly.1 : rename
    end
```

