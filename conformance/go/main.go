package main

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"runtime/debug"
	"sort"
	"strings"

	"github.com/compose-spec/compose-go/v2/cli"
	"github.com/compose-spec/compose-go/v2/dotenv"
	"github.com/compose-spec/compose-go/v2/types"
	"github.com/docker/cli/pkg/kvfile"
)

type fixture struct {
	ID                 string             `json:"id"`
	Source             string             `json:"source"`
	Environment        map[string]string  `json:"environment"`
	ServiceEnvironment map[string]*string `json:"service_environment"`
}

type request struct {
	Cases []fixture `json:"cases"`
}

type observation map[string]any

type response struct {
	Runtime  map[string]string                 `json:"runtime"`
	Profiles map[string]map[string]observation `json:"profiles"`
}

var profileNames = []string{
	"compose-parser-default",
	"compose-project-environment",
	"compose-service-default",
	"compose-service-raw",
	"docker-raw-no-lookup",
	"docker-raw-lookup",
}

func verifyDependencies() {
	info, ok := debug.ReadBuildInfo()
	if !ok {
		panic("Go build provenance unavailable")
	}
	pins := map[string]string{
		"github.com/compose-spec/compose-go/v2": "v2.16.1",
		"github.com/docker/cli":                 "v29.8.2+incompatible",
	}
	for _, dependency := range info.Deps {
		if version, found := pins[dependency.Path]; found {
			if dependency.Replace != nil || dependency.Version != version {
				panic("unexpected dependency: " + dependency.Path)
			}
			delete(pins, dependency.Path)
		}
	}
	if len(pins) != 0 {
		panic("missing pinned Go dependency")
	}
}

// Register the same public-API integration as Docker Compose v5.6.0's
// cmd/compose/compose.go rawEnv: kvfile parsing followed by ordered assignments.
// This is adapter glue, not a replacement parser or service-role resolver.
// https://github.com/docker/compose/blob/42f48072bbf92ee9b0e43f9fdf2008d03546e7ca/cmd/compose/compose.go
func registerRawFormat() {
	dotenv.RegisterFormat("raw", func(reader io.Reader, _ string, variables map[string]string, lookup func(string) (string, bool)) error {
		lines, err := kvfile.ParseFromReader(reader, lookup)
		if err != nil {
			return err
		}
		for _, line := range lines {
			key, value, _ := strings.Cut(line, "=")
			variables[key] = value
		}
		return nil
	})
}

func lookup(environment map[string]string) func(string) (string, bool) {
	return func(key string) (string, bool) {
		value, found := environment[key]
		return value, found
	}
}

func environmentList(environment map[string]string) []string {
	values := make([]string, 0, len(environment))
	for key, value := range environment {
		values = append(values, key+"="+value)
	}
	sort.Strings(values)
	return values
}

func observeService(path string, input fixture, format string) observation {
	project := types.Project{
		Environment: types.Mapping(input.Environment),
		Services: types.Services{
			"oracle": {
				Name: "oracle",
				ContainerSpec: types.ContainerSpec{
					Environment: types.MappingWithEquals(input.ServiceEnvironment),
					EnvFiles:    []types.EnvFile{{Path: path, Required: true, Format: format}},
				},
			},
		},
	}
	resolved, err := project.WithServicesEnvironmentResolved(true)
	if err != nil {
		return observation{"values": nil, "error": true}
	}
	return observation{"values": resolved.Services["oracle"].Environment, "error": false}
}

func observe(input fixture) (map[string]observation, error) {
	directory, err := os.MkdirTemp("", "adotenv-conformance-go-")
	if err != nil {
		return nil, err
	}
	defer os.RemoveAll(directory)
	path := filepath.Join(directory, "synthetic.env")
	if err := os.WriteFile(path, []byte(input.Source), 0600); err != nil {
		return nil, err
	}
	results := make(map[string]observation, len(profileNames))
	values, parseErr := dotenv.ParseWithLookup(strings.NewReader(input.Source), lookup(input.Environment))
	results["compose-parser-default"] = observation{"values": values, "error": parseErr != nil}

	options, projectErr := cli.NewProjectOptions(nil,
		cli.WithEnv(environmentList(input.Environment)), cli.WithEnvFiles(path), cli.WithDotEnv)
	if projectErr != nil {
		results["compose-project-environment"] = observation{"values": nil, "error": true}
	} else {
		results["compose-project-environment"] = observation{"values": options.Environment, "error": false}
	}
	results["compose-service-default"] = observeService(path, input, "")
	results["compose-service-raw"] = observeService(path, input, "raw")

	for _, useLookup := range []bool{false, true} {
		profile := "docker-raw-no-lookup"
		var callback func(string) (string, bool)
		if useLookup {
			profile = "docker-raw-lookup"
			callback = lookup(input.Environment)
		}
		lines, rawErr := kvfile.ParseFromReader(strings.NewReader(input.Source), callback)
		if lines == nil {
			lines = []string{}
		}
		results[profile] = observation{"lines": lines, "error": rawErr != nil}
	}
	return results, nil
}

func run() error {
	verifyDependencies()
	registerRawFormat()
	var input request
	if err := json.NewDecoder(os.Stdin).Decode(&input); err != nil {
		return err
	}
	output := response{
		Runtime:  map[string]string{"language": runtime.Version(), "package": "compose-go v2.16.1; docker/cli v29.8.2+incompatible"},
		Profiles: make(map[string]map[string]observation, len(profileNames)),
	}
	for _, profile := range profileNames {
		output.Profiles[profile] = make(map[string]observation, len(input.Cases))
	}
	for _, fixture := range input.Cases {
		results, err := observe(fixture)
		if err != nil {
			return fmt.Errorf("fixture %s: %w", fixture.ID, err)
		}
		for profile, result := range results {
			output.Profiles[profile][fixture.ID] = result
		}
	}
	return json.NewEncoder(os.Stdout).Encode(output)
}

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
