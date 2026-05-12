# tppss-rs : rust version of TPPSS

Task 1 :

Port the python lib / cli app tppss to Rust (latest 2024 edition)

The python code is at : /Users/guilhem/Documents/projects/github/tppss

Follow those considerations:
- have both a binary (for the cli) and a lib (for the code that does the work). The lib will also be used in a separate package that will serve the functionality as a web API (see below). You can use a workspace with 2 packages
- For the Geotiff reading (both local and remote, especially Google Cloud Storage), use async-tiff. Document what option to enable read from GCS or S3
- use clap for the CLI.
- use ndarray for the computations. Define operators so it is a bit clean (similar to numpy), if it makes sense
- do not use non-rust native library (like GDAL or PROJ)
- you can use epsg-utils or miniproj
- needs to work at least on macos (ARM) + linux (on Cloud Run x64)

Document in the README how to build + how to run eg how to use a local Geotiff or geotiff hosted on GCS.
You can use the local Geotiff for testing (read only) : /Users/guilhem/Documents/projects/dtm/dem_wgs84_b.tif . See /Users/guilhem/Documents/projects/github/tppss/.vscode/launch.json for launches
Compare the results of the python version with the result of the rust version so they match.
Do not address the TODO for now. You can add them to a section in the README instead of scattered through the code.

Task 2 :

Also : setup a new package of a web API that will use the TPPSS-rs lib (mentioned above) :
the folder where to create it is at 
/Users/guilhem/Documents/projects/github/tppss-rs_server

Follow the existing equivalent TPPSS web server at :
/Users/guilhem/Documents/projects/github/tppss_web/

That folder actually contains the code for both the server and the web client (JS) : do not deal with the web client, only with the server (which is FastAPI in the Python version). So leave the www folder alone and do not port it. The server code is in folder tppss_api. tppss-rs_server will contain only the rust server so it can be setup as a standard rust standalone project.
Create a new rust package, setup the dependencies, write the code : it should accept the same things and return the same things as the python server
The server will use the lib of the tppss-rs : but using the repo from the project https://github.com/gvellut/tppss-rs ( I will patch .cargo/config.toml  on my machine to use the local folder ie [patch."https://github.com/user/my-lib.git"]
my-lib = { path = "../my-lib" })
Use Axum async library.
Make it run locally (so can debug) or deployable on Cloud Run with configuration. Document in README.md
Port the Dockerfile to build and use the rust server. Adapt the Github Workflow configuration to deploy on Cloud Run (like the Python server). Clean it so no interference from the non existing (in tppss-rs_server) www web client so simplify. Also simplify : there are no staging or prod branches : only the main branch. The deployment is done with tags that start with staging or prod : see /Users/guilhem/Documents/projects/github/webcams_maps/.github/workflows/www-prod.yml Also you can use docker buildx with one call for tag and push instead of the multiple jobs so you dont have to use the exact same sequence of calls to build the image
Document in README.md
Also deal with the kind-of-rate limiting MAX_CONCURRENTS : the Cloud Run project will be deployed on a single instance (with the standard max 80 requests of Cloud Run) with no scaling to more instances. So this is how the process is not starved for CPU. The additional requeests wait in line. I read to use task::spawn_blocking and tower middleware with layer(ConcurrencyLimitLayer::new(MAX_CONCURRENTS)). But find out if OK