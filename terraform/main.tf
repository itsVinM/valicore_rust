terraform {
  required_providers {
    docker = {
      source  = "kreuzwerker/docker"
      version = "~> 3.0"
    }
  }
}

provider "docker" {}

resource "docker_image" "valicore" {
  name = "valicore:latest"
  build {
    path = ".."
  }
}

resource "docker_container" "valicore" {
  name  = "valicore"
  image = docker_image.valicore.image_id
  command = ["valicore", "resources"]
  rm     = true
}
