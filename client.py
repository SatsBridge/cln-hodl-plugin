import asyncio
from concurrent.futures import ThreadPoolExecutor
import functools
import time
from pathlib import Path
import grpc

import primitives_pb2
import node_pb2
import node_pb2_grpc
import plugin_pb2
import plugin_pb2_grpc


_executor = ThreadPoolExecutor(1)

p = Path("/home/ilya/.lightningd/testnet")
cert_path = p / "client.pem"
key_path = p / "client-key.pem"
ca_cert_path = p / "ca.pem"

creds = grpc.ssl_channel_credentials(
    root_certificates=ca_cert_path.open('rb').read(),
    private_key=key_path.open('rb').read(),
    certificate_chain=cert_path.open('rb').read()
)


def _submit(func, *args):
    func_partial = functools.partial(func, *args)
    return loop.run_in_executor(_executor, func_partial)


async def main():

    with grpc.secure_channel('localhost:19111',creds, options=(('grpc.ssl_target_name_override', 'cln'),)) as channel:
        stub = node_pb2_grpc.NodeStub(channel)

        print('Send request Getinfo')
        await _submit(stub.Getinfo, node_pb2.GetinfoResponse())

        print('Request successful')


    with grpc.secure_channel('localhost:19112',creds, options=(('grpc.ssl_target_name_override', 'cln'),)) as channel:
        stub = plugin_pb2_grpc.PluginStub(channel)
        print('Send request Hodl Invoice')

        await _submit(stub.HodlInvoice, plugin_pb2.HodlInvoiceRequest(
            amount_msat=100,
            description="hello",
            label="lbl1",
            expiry=7200,
            cltv=24
        ), plugin_pb2.HodlInvoiceResponse())

        print('Request successful')


if __name__ == '__main__':
    with grpc.secure_channel('localhost:19112',creds, options=(('grpc.ssl_target_name_override', 'cln'),)) as channel:
        stub = plugin_pb2_grpc.PluginStub(channel)
        print('Send request Hodl Invoice')
        inv = stub.Ping(plugin_pb2.PluginPingRequest( message="hello"))
        print(inv)


    with grpc.secure_channel('localhost:19112',creds, options=(('grpc.ssl_target_name_override', 'cln'),)) as channel:
        stub = plugin_pb2_grpc.PluginStub(channel)
        print('Send request Hodl Invoice')
        inv = stub.HodlInvoice(plugin_pb2.HodlInvoiceRequest( amount_msat=100, description="hello", label="lbl1"))
        print(inv)

    loop = asyncio.get_event_loop()
    loop.run_until_complete(main())
    loop.close()
