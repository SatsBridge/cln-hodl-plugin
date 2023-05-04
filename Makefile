stubs:
	python -m grpc.tools.protoc -I/usr/local/include -I./proto -I. --python_out=./ --grpc_python_out=./ ./proto/plugin.proto
