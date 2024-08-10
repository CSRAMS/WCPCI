{
  python3,
  rustc,
  ghc,
  ocaml,
  gcc,
  go,
  R,
  julia-bin,
  lua,
  perl,
  kotlin,
  ruby,
  php,
  fsharp,
  bash,
  coreutils,
  typescript,
  nodejs,
  openjdk,
  mono,
  writers,
}:
# TODO: The backend seems to rebuild when changing this?
writers.writeTOML "rocket.toml" {
  release = {
    cli_colors = false;
    timezone = "America/New_York";
    port = 443;
    # ip_header = "X-Forwarded-For";
    address = "0.0.0.0";
    url = "https://codingcomp.cs.wcupa.edu"; # This should *not* have a trailing slash

    # TODO(Spoon): Do the data things
    saml = {
      entity_id = "urn:wcu:sp:wcpc";
      idp_meta_url = "https://mocksaml.com/api/namespace/wcpc_testing_provider/saml/metadata";
      contact_person = "Ben";
      contact_email = "bwc9876@example.org";
      contact_telephone = "555-555-5555";
      organization = "West Chester University Computer Science Department";

      attrs = {
        display_name = "firstName";
        email = "email";
      };
    };

    # When running in a container, this will be overridden.
    # Only change if you are running without the container
    # This can be an absolute path
    databases.sqlite_db.url = "database.sqlite";

    oauth = {
      github.provider = "GitHub";
      google.provider = "Google";
    };

    run = {
      max_program_length = 100000;
      default_language = "python";

      isolation = {
        workers_parent = "/tmp";
        # Putting ls here but it'll include the entire dir in PATH
        include_bins = ["${gcc}/bin/cc" "${coreutils}/bin/ls"];
        bind_mounts = [
          {src = "/nix/store";}
          {src = "/bin/sh";}
          {src = "/usr/bin/env";}
        ];
      };

      languages = {
        bash = {
          display = {
            name = "Bash";
            default_code = ''
              echo "Hello, World!"
            '';
            monaco_contribution = "shell";
          };
          runner = {
            file_name = "main.sh";
            run_cmd = {
              binary = "${bash}/bin/bash";
              args = ["./main.sh"];
            };
          };
        };
        python = {
          display = {
            name = "Python";
            default_code = ''
              print("Hello, World!")
            '';
            monaco_contribution = "python";
          };
          runner = {
            file_name = "main.py";
            run_cmd = {
              binary = "${python3}/bin/python3";
              args = ["./main.py"];
            };
          };
        };
        rust = {
          display = {
            name = "Rust";
            default_code = ''
              fn main() {
                  println!("Hello, World!");
              }
            '';
            monaco_contribution = "rust";
          };
          runner = {
            file_name = "main.rs";
            compile_cmd = {
              binary = "${rustc}/bin/rustc";
              args = ["main.rs" "-o" "main"];
            };
            run_cmd = {binary = "./main";};
          };
        };
        haskell = {
          display = {
            name = "Haskell";
            default_code = ''
              main = putStrLn "Hello, World!"
            '';
            monaco_contribution = "haskell";
          };
          runner = {
            file_name = "main.hs";
            compile_cmd = {
              binary = "${ghc}/bin/ghc";
              args = ["main.hs"];
            };
            run_cmd = {binary = "./main";};
          };
        };
        ocaml = {
          display = {
            name = "OCaml";
            default_code = ''
              print_endline "Hello, World!"
            '';
            monaco_contribution = "ocaml";
          };
          runner = {
            file_name = "main.ml";
            compile_cmd = {
              binary = "${ocaml}/bin/ocamlc";
              args = ["main.ml" "-o" "main"];
            };
            run_cmd = {binary = "./main";};
          };
        };
        typescript = {
          display = {
            name = "TypeScript / JavaScript";
            default_code = ''
              console.log("Hello, World!");
            '';
            tabler_icon = "brand-typescript";
            monaco_contribution = "typescript";
          };
          runner = {
            file_name = "main.ts";
            compile_cmd = {
              binary = "${typescript}/bin/tsc";
              args = ["main.ts"];
            };
            run_cmd = {
              binary = "${nodejs}/bin/node";
              args = ["main.js"];
            };
          };
        };
        java = {
          display = {
            name = "Java";
            default_code = ''
              public class Main {
                  public static void main(String[] args) {
                      System.out.println("Hello, World!");
                  }
              }
            '';
            monaco_contribution = "java";
          };
          runner = {
            file_name = "Main.java";
            compile_cmd = {
              binary = "${openjdk}/bin/javac";
              args = ["Main.java"];
            };
            run_cmd = {
              binary = "${openjdk}/bin/java";
              args = ["Main"];
            };
          };
        };
        c = {
          display = {
            name = "C";
            default_code = ''
              #include <stdio.h>

              int main() {
                  printf("Hello, World!\\n");
                  return 0;
              }
            '';
            monaco_contribution = "c";
          };
          runner = {
            file_name = "main.c";
            compile_cmd = {
              binary = "${gcc}/bin/gcc";
              args = ["main.c" "-o" "main"];
            };
            run_cmd = {binary = "./main";};
          };
        };
        cpp = {
          display = {
            name = "C++";
            default_code = ''
              #include <iostream>

              int main() {
                  std::cout << "Hello, World!" << std::endl;
                  return 0;
              }
            '';
            devicon_icon = "cplusplus";
            monaco_contribution = "cpp";
          };
          runner = {
            file_name = "main.cpp";
            compile_cmd = {
              binary = "${gcc}/bin/g++";
              args = ["main.cpp" "-o" "main"];
            };
            run_cmd = {binary = "./main";};
          };
        };
        csharp = {
          display = {
            name = "C#";
            default_code = ''
              public class Program
              {
                  public static void Main(string[] args)
                  {
                      System.Console.WriteLine("Hello, World!");
                  }
              }
            '';
            monaco_contribution = "csharp";
          };
          runner = {
            file_name = "Program.cs";
            compile_cmd = {
              binary = "${mono}/bin/mcs";
              args = ["Program.cs"];
            };
            run_cmd = {
              binary = "${mono}/bin/mono";
              args = ["Program.exe"];
            };
          };
        };
        go = {
          display = {
            name = "Go";
            default_code = ''
              package main

              import "fmt"

              func main() {
                  fmt.Println("Hello, World!")
              }
            '';
            monaco_contribution = "go";
          };
          runner = {
            file_name = "main.go";
            compile_cmd = {
              binary = "${go}/bin/go";
              args = ["build" "-o" "main" "main.go"];
            };
            run_cmd = {binary = "./main";};
          };
        };
        lua = {
          display = {
            name = "Lua";
            default_code = ''
              print("Hello, World!")
            '';
            monaco_contribution = "lua";
          };
          runner = {
            file_name = "main.lua";
            run_cmd = {
              binary = "${lua}/bin/lua";
              args = ["./main.lua"];
            };
          };
        };
        perl = {
          display = {
            name = "Perl";
            default_code = ''
              print "Hello, World!";
            '';
            monaco_contribution = "perl";
          };
          runner = {
            file_name = "main.pl";
            run_cmd = {
              binary = "${perl}/bin/perl";
              args = ["./main.pl"];
            };
          };
        };
        ruby = {
          display = {
            name = "Ruby";
            default_code = ''
              puts "Hello, World!"
            '';
            monaco_contribution = "ruby";
          };
          runner = {
            file_name = "main.rb";
            run_cmd = {
              binary = "${ruby}/bin/ruby";
              args = ["./main.rb"];
            };
          };
        };
        php = {
          display = {
            name = "PHP";
            default_code = ''
              <?php
              echo "Hello, World!";
              ?>
            '';
            monaco_contribution = "php";
          };
          runner = {
            file_name = "main.php";
            run_cmd = {
              binary = "${php}/bin/php";
              args = ["./main.php"];
            };
          };
        };
        fsharp = {
          display = {
            name = "F#";
            default_code = ''
              open System

              [<EntryPoint>]
              let main argv =
                  printfn "Hello, World!"
                  0
            '';
            monaco_contribution = "fsharp";
          };
          runner = {
            file_name = "main.fs";
            compile_cmd = {
              binary = "${mono}/bin/fsharpc";
              args = ["--standalone" "main.fs"];
            };
            run_cmd = {
              binary = "${mono}/bin/mono";
              args = ["main.exe"];
            };
          };
        };
        r = {
          display = {
            name = "R";
            default_code = ''
              cat("Hello, World!")
            '';
            monaco_contribution = "r";
          };
          runner = {
            file_name = "main.r";
            run_cmd = {
              binary = "${R}/bin/Rscript";
              args = ["--vanilla" "main.r"];
            };
          };
        };
        julia = {
          display = {
            name = "Julia";
            default_code = ''
              println("Hello, World!")
            '';
            monaco_contribution = "julia";
          };
          runner = {
            file_name = "main.jl";
            run_cmd = {
              binary = "${julia-bin}/bin/julia";
              args = ["main.jl"];
            };
          };
        };
        kotlin = {
          display = {
            name = "Kotlin";
            default_code = ''
              fun main() {
                  println("Hello, World!")
              }
            '';
            monaco_contribution = "kotlin";
          };
          runner = {
            file_name = "main.kt";
            compile_cmd = {
              binary = "${kotlin}/bin/kotlinc";
              args = ["main.kt" "-include-runtime" "-d" "main.jar"];
            };
            run_cmd = {
              binary = "${openjdk}/bin/java";
              args = ["-jar" "main.jar"];
            };
          };
        };
      };
    };
  };
}
