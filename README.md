# Log File Processor

A Rust experiment in quickly parsing the specialist types of log files
we have at work (there is no commercial IP in this repo, it is all generic
code).

The aim was to go as fast as possible while keeping clear code.


# TODO
* [ ] Auto-open the consolidated.csv.
* [ ] Excel has trouble with the LogDate string.
* [ ] Perf: Figure out how to do profiling.
* [ ] Perf: Is it faster to write everything to RAM first? We could parallelize that.
* [ ] Tools: figure out how to run rustfmt in VS Code.
* [ ] Tools: figure out how to run clippy in VS Code
* [ ] Allow custom regex extractors for columns.
* [ ] Filter: from/to dates
    - I have added the raw strings to the Arguments.
    - I now need to add to the profile.
    - Then do the From for Configuration
    - Then do the get_config method
    - Finally apply the date filter to the parsed line.
* [ ] Filter: column is non-blank, e.g. for call recorder execution time
* [ ] Filter: column matches a regex, ANY column matches a regex. DOES NOT MATCH, e.g. to get rid of heartbeats.
* [ ] Rewrite using nom!
* [ ] Write some macros to help with the ugliness of the tests
* [ ] Get a better assertions library


Some sysrefs seen:
Q2952601,Q2952601,Q2967281,Q2952601,Q2967281,Q2975135,Q2967281,Q2970508,Q2967281


# Interesting Crates
* rpassword (for reading)
* https://github.com/hwchen/keyring-rs ?
* Persistent secure credentials

