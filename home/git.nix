{ ... }:
{
  programs.git = {
    enable = true;
    settings = {
      user = {
        name = "chelokot";
        email = "andrey.vlasenko.work@gmail.com";
      };
      credential = {
        "https://github.com".helper = [
          ""
          "!gh auth git-credential"
        ];
        "https://gist.github.com".helper = [
          ""
          "!gh auth git-credential"
        ];
        "https://gitlab.com".helper = [
          ""
          "!glab auth git-credential"
        ];
      };
    };
    includes = [
      {
        condition = "gitdir:~/Documents/Projects/Oriane/";
        contents = {
          user = {
            name = "Andrii Vlasenko";
            email = "andrii@oriane.xyz";
            signingKey = "6B19D7E4D979C3B2";
          };
          commit.gpgSign = true;
          gpg.program = "gpg";
        };
      }
    ];
  };
}
