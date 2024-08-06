{
  python3,
  rustc,
  ghc,
  gcc,
  bash,
  typescript,
  nodejs,
  openjdk,
  mono,
  writers,
}:
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

      languages = {
        bash = {
          display = {
            name = "Bash";
            default_code = ''
              echo "Hello, World!"
            '';
            tabler_icon = "terminal";
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
            tabler_icon = "brand-python";
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
            tabler_icon = "brand-rust";
            monaco_contribution = "rust";
          };
          runner = {
            file_name = "main.rs";
            include_bins = ["${gcc}/bin/cc"];
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
            tabler_icon = "lambda";
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
            tabler_icon = "coffee";
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
            tabler_icon = "brand-c-sharp";
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
      };
    };
  };
}
