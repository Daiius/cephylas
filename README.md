# Cephylas Telemetrer
Simple server load logging system for Linux systems,
written in Rust.

Cephylas comes from "cephonodes hylas", a kind of moth with clear wings.

## Memo
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

## Log summarization
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

