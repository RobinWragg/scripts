import socket
import time

id = 'pikachu'

server_address = ('bmo.local', 8080)  # Replace with your server's IP and port
interval = 2

while True:
    try:
        # Connect to the server
        print("Connecting to server...")
        client_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        client_socket.settimeout(1)
        client_socket.connect(server_address)
        print("Connected to server")

        # Send data
        message = '{"id":"' + id + '"}'
        client_socket.send(message.encode())  # Encode the message to bytes
        print("Message sent:", message)

        # Receive response from the server
        response = client_socket.recv(1024)  # Buffer size of 1024 bytes
        print("Response from server:", response.decode())  # Decode the response to string

    except socket.timeout:
        print("Connection timed out")
    except Exception as e:
        print("An error occurred:", e)

    finally:
        client_socket.close()
        print("Socket closed")
    time.sleep(interval)
