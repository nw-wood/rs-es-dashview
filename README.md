# Logstash ESQL Query Viewer

A simple terminal viewer for monitoring Logstash HTTP poller output from Elasticsearch ESQL queries. Built as an exploratory project to test ESQL functionality.

## What it Does

- Runs a local server that receives JSON data from Logstash's HTTP poller
- Displays the results in a terminal UI
- Updates automatically when new data arrives
- Shows common fields like timestamps, host info, and agent IDs

## Quick Start

1. Configure Logstash to send HTTP poller output to `http://127.0.0.1:33433/data`
2. Run the application: `cargo run`
3. Press 'q' to quit

## Example Logstash Config

```ruby
input {
    beats {
        port => 5044
        tags => ["beats_input"]
    }

    http_poller {
        tags => ["http_poller"]
        urls => {
            es_search => {
                method => post
                url => "https://localhost:9200/_query"
                body => '{
                    "query": "FROM logs-generic-default | SORT @timestamp DESC | LIMIT 1"
                }'
                headers => {
                    "Content-Type" => "application/json"
                }
            }
        }
        schedule => { "every" => "5s" }
        codec => "json"
    }
}

output {
    if "beats_input" in [tags] {
        stdout {
            codec => rubydebug
        }
        elasticsearch {
            hosts => ["https://localhost:9200"]
        }
    }

    if "http_poller" in [tags] {
        http {
            url => "http://localhost:33433/data"
            http_method => "post"
            format => "json"
            content_type => "application/json"
        }
    }
}
```