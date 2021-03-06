# Copyright 2013 10gen Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
# http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Preprocessor versioning info
MAJOR_VERS := $(shell mongo --version | grep -o '[0-9].[0-9].[0-9]' | cut -d'.' -f1)
MINOR_VERS := $(shell mongo --version | grep -o '[0-9].[0-9].[0-9]' | cut -d'.' -f2)
PATCH_VERS := $(shell mongo --version | grep -o '[0-9].[0-9].[0-9]' | cut -d'.' -f3)

# Rust compilation
RC = rustc
RDOC = rustdoc
RDOCFLAGS = --output-style doc-per-mod --output-format markdown
FLAGS = -Z debug-info -L ./bin -D unused-unsafe -A unnecessary-allocation --cfg major=$(MAJOR_VERS) --cfg minor=$(MINOR_VERS) --cfg patch=$(PATCH_VERS) $(TOOLFLAGS)

# C compilation
CC = gcc
AR = ar rcs
CFLAGS = -c -g -Wall -Werror

# Programs and utilities
RM = rm
RMDIR = rmdir -p
MKDIR = mkdir -p

# Directories
SRC = ./src
LIB = ./lib
BSONDIR = ./src/libbson
MONGODIR = ./src/libmongo
GRIDDIR = ./src/libgridfs
UTILDIR = ./src/tools
EXDIR = ./examples
BIN = ./bin
TEST = ./test
DOCS = ./docs

# Variables
MONGOTEST = 0
TOOLFLAGS =

.PHONY: test

all: bin libs util bson mongo gridfs

bin:
	$(MKDIR) bin
	$(MKDIR) test

util: $(UTILDIR)/*
	$(RC) $(FLAGS) --lib --out-dir $(BIN) $(UTILDIR)/tools.rs

libs: $(LIB)/md5.c
	$(CC) $(CFLAGS) -o $(BIN)/md5.o $(LIB)/md5.c
	$(AR) $(BIN)/libmd5.a $(BIN)/md5.o

bson: $(BSONDIR)/*
	$(RC) $(FLAGS) --lib --out-dir $(BIN) $(BSONDIR)/bson.rs

mongo: $(MONGODIR)/*
	$(RC) $(FLAGS) --lib --out-dir $(BIN) $(MONGODIR)/mongo.rs

gridfs: $(GRIDDIR)/*
	$(RC) $(FLAGS) --lib --out-dir $(BIN) $(GRIDDIR)/gridfs.rs

test: $(BSONDIR)/bson.rs $(MONGODIR)/mongo.rs $(MONGODIR)/test/test.rs
	$(RC) $(FLAGS) --test -o $(TEST)/tool_test $(UTILDIR)/tools.rs
	$(RC) $(FLAGS) --test -o $(TEST)/bson_test $(BSONDIR)/bson.rs
	$(RC) $(FLAGS) --test -o $(TEST)/mongo_test $(MONGODIR)/mongo.rs
	$(RC) $(FLAGS) --test -o $(TEST)/driver_test $(MONGODIR)/test/test.rs
	$(RC) $(FLAGS) --test -o $(TEST)/grid_test $(GRIDDIR)/test/test.rs

check: test
ifeq ($(MONGOTEST),1)
	$(TEST)/tool_test
	$(TEST)/bson_test
	$(TEST)/mongo_test
	$(TEST)/driver_test
	$(TEST)/grid_test
else
	$(TEST)/tool_test
	$(TEST)/bson_test
	$(TEST)/mongo_test
endif

bench: test
	$(TEST)/bson_test --bench

ex: $(EXDIR)/*
	$(RC) $(FLAGS) $(EXDIR)/bson_demo.rs
	$(RC) $(FLAGS) $(EXDIR)/mongo_demo.rs
	$(RC) $(FLAGS) $(EXDIR)/rs_demo.rs
	$(RC) $(FLAGS) $(EXDIR)/tutorial.rs

doc: $(BSONDIR)/*.rs $(MONGODIR)/*
	$(MKDIR) docs
	$(MKDIR) docs/bson
	$(MKDIR) docs/mongo
	$(MKDIR) docs/gridfs
	$(RDOC) $(RDOCFLAGS) --output-dir $(DOCS)/bson $(BSONDIR)/bson.rs
	$(RDOC) $(RDOCFLAGS) --output-dir $(DOCS)/mongo $(MONGODIR)/mongo.rs
	$(RDOC) $(RDOCFLAGS) --output-dir $(DOCS)/gridfs $(GRIDDIR)/gridfs.rs

clean:
	$(RM) -rf $(TEST)
	$(RM) -rf $(BIN)

tidy:
	for f in `find . -name '*.rs'`; do perl -pi -e "s/[ \t]*$$//" $$f; done
