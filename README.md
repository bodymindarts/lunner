# Lunner

a simple daemon process for managing high-availability where only a single instance is supposed to actually execute but there may be multiple stand-bys.
The l(eader)-runner processes create and manage a table in a configured pg database.
This table is used to identify which instance of the process is currently considered the `leader` (vs `standby`). On transitions from `leader` -> `standby` and vica-versa hooks are executed that can be used to start / stop other processes or trigger other arbitrary effects.

## Config:

```
id: lunner                   ## Identity for this process (must be unique for each instance)
leader_timeout_seconds: 30   ## How long until standby processes consider the leader process unresponsive
postgres:                        
  connection: postgres://postgres:postgres@127.0.0.1:5432/postgres    ## pg connection url
hooks:
  become_leader:                                                      ## Hook that will be executed when a process becomes the leader
    cmd: "bin/leader_process"
    args: []
  become_standby:                                                     ## Hook that will be executed on start-up and when becoming standby
    cmd: "echo"
    args:
    - "hook standby"
```
