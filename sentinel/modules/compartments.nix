{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.sentinel.compartments;
  backend = cfg.backend;
in
{
  options.sentinel.compartments = {
    enable = mkEnableOption "seL4-based compartment runtime";
    backend = mkOption {
      type = types.enum [ "crosvm" "qemu" "kata" ];
      default = "crosvm";
      description = "VMM/container backend";
    };
    # Default resource limits
    defaults = {
      cpu = mkOption {
        type = types.int;
        default = 2;
        description = "Default vCPU count";
      };
      memory = mkOption {
        type = types.str;
        default = "4G";
        description = "Default memory";
      };
      disk = mkOption {
        type = types.str;
        default = "10G";
        description = "Default disk size";
      };
    };
    # Network policies
    network = {
      enable = mkEnableOption "Compartment networking";
      defaultPolicy = mkOption {
        type = types.enum [ "none" "egress" "ingress" "full" ];
        default = "none";
        description = "Default network policy";
      };
      vlans = mkOption {
        type = types.attrsOf (types.attrsOf types.str);
        default = {
          mgmt = { id = "10"; cidr = "10.100.0.0/24"; };
          tool = { id = "20"; cidr = "10.101.0.0/24"; };
          data = { id = "30"; cidr = "10.102.0.0/24"; };
        };
      };
    };
    # GPU/NPU passthrough
    gpu = {
      enable = mkEnableOption "GPU/NPU passthrough";
      devices = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = "GPU/NPU device IDs to expose";
      };
      mediated = mkOption {
        type = types.bool;
        default = true;
        description = "Use mediated devices (vGPU)";
      };
    };
    # Storage
    storage = {
      pool = mkOption {
        type = types.path;
        default = "/var/lib/sentinel/compartments";
        description = "Storage pool for compartment disks";
      };
      defaultSize = mkOption {
        type = types.str;
        default = "10G";
        description = "Default disk image size";
      };
    };
  };

  config = mkIf cfg.enable {
    # Install VMM/container runtime
    environment.systemPackages = with pkgs; [
      (if backend == "crosvm" then crosvm else if backend == "qemu" then qemu else kata-containers)
      virtiofsd
      cloud-hypervisor
      # Management tools
      libvirt
      virt-manager
      virt-install
      virsh
      # Network
      bridge-utils
      vlan
      iproute2
      # Storage
      qemu-img
      nbd
      # Monitoring
      prometheus-node-exporter
    ];

    # Enable libvirt
    virtualisation.libvirtd = {
      enable = true;
      # Run as root for hardware access
      # But we'll use unprivileged where possible
    };

    # crosvm-specific configuration
    mkIf (backend == "crosvm") {
      # crosvm needs KVM and vfio
      boot.kernelModules = [ "kvm" "kvm_intel" "kvm_amd" "vfio" "vfio_pci" "vfio_iommu_type1" "vfio_mdev" ];

      # crosvm service for managing compartments
      systemd.services.sentinel-crosvm = {
        description = "Sentinel crosvm Compartment Manager";
        wantedBy = [ "multi-user.target" ];
        after = [ "libvirtd.service" "network-online.target" ];
        serviceConfig = {
          ExecStart = "${pkgs.writeScriptBin "sentinel-crosvm-manager" ''}
            #!${pkgs.runtimeShell}
            # Daemon that manages crosvm compartments via gRPC
            exec ${pkgs.crosvm}/bin/crosvm run --disable-sandbox=false --cid 3 \\
              --memory ${toString cfg.defaults.memory} \\
              --cpus ${toString cfg.defaults.cpu} \\
              --rwdisk /var/lib/sentinel/compartments/root.img \\
              --shared-dir /nix/store:/nix/store:ro \\
              --shared-dir /var/lib/sentinel/models:/models:ro \\
              --gpu auto \\
              --wayland-socket \\
              --vsock-port 10000 \\
              ${config.boot.kernelPackages.kernel}/Image \\
              -p "console=hvc0 root=/dev/vda1 ro"
          '';
          User = "sentinel";
          Group = "kvm";
          SupplementaryGroups = [ "kvm" "render" "video" ];
          CapabilityBoundingSet = "CAP_SYS_ADMIN CAP_SYS_RESOURCE CAP_DAC_OVERRIDE CAP_MKNOD CAP_NET_ADMIN CAP_SYS_MODULE";
          AmbientCapabilities = "CAP_SYS_ADMIN CAP_SYS_RESOURCE CAP_DAC_OVERRIDE CAP_MKNOD CAP_NET_ADMIN CAP_SYS_MODULE";
          NoNewPrivileges = false;
        };
      };
    };

    # QEMU-specific configuration
    mkIf (backend == "qemu") {
      # QEMU with KVM
      boot.kernelModules = [ "kvm" "kvm_intel" "kvm_amd" "vfio" "vfio_pci" "vfio_iommu_type1" "virtio_net" "virtio_blk" "virtio_scsi" "virtio_balloon" "virtio_rng" "virtio_vsock" ];

      # QEMU compartment manager
      systemd.services.sentinel-qemu = {
        description = "Sentinel QEMU Compartment Manager";
        wantedBy = [ "multi-user.target" ];
        after = [ "libvirtd.service" ];
        serviceConfig = {
          ExecStart = "${pkgs.writeScriptBin "sentinel-qemu-manager" ''}
            #!${pkgs.runtimeShell}
            # Daemon that manages QEMU compartments via libvirt
            exec ${pkgs.libvirt}/bin/libvirtd --daemon --listen
          '';
          User = "root";
        };
      };
    };

    # Kata Containers configuration
    mkIf (backend == "kata") {
      # Kata Containers runtime
      virtualisation.kata-containers = {
        enable = true;
        # Use QEMU as hypervisor
        hypervisor = "qemu";
        # Kernel for kata VMs
        kernel = config.boot.kernelPackages.kernel;
        # Initrd for kata VMs
        initrd = pkgs.kata-containers.initrd;
      };

      # Containerd with Kata runtime
      virtualisation.containerd = {
        enable = true;
        settings = {
          version = 2;
          plugins = {
            "io.containerd.grpc.v1.cri" = {
              containerd = {
                default_runtime_name = "kata";
                runtimes = {
                  kata = {
                    runtime_type = "io.containerd.kata.v2";
                    privileged_without_host_devices = true;
                  };
                  runc = {
                    runtime_type = "io.containerd.runc.v2";
                  };
                };
              };
            };
          };
        };
      };
    };

    # Network bridges for compartments
    networking.bridges = {
      "sentinel-mgmt" = {
        interfaces = [ ];
        ip4 = cfg.network.vlans.mgmt.cidr;
      };
      "sentinel-tool" = {
        interfaces = [ ];
        ip4 = cfg.network.vlans.tool.cidr;
      };
      "sentinel-data" = {
        interfaces = [ ];
        ip4 = cfg.network.vlans.data.cidr;
      };
    };

    # VLAN interfaces
    networking.vlans = lib.mapAttrs (name: vlan: {
      interface = "sentinel-${name}";
      id = builtins.toInt vlan.id;
    }) cfg.network.vlans;

    # Firewall rules for compartment networks
    networking.firewall.extraCommands = ''
      # Management VLAN - restricted
      iptables -A FORWARD -i sentinel-mgmt -o sentinel-tool -j DROP
      iptables -A FORWARD -i sentinel-mgmt -o sentinel-data -j DROP
      iptables -A FORWARD -i sentinel-tool -o sentinel-mgmt -j DROP
      iptables -A FORWARD -i sentinel-tool -o sentinel-data -j DROP
      iptables -A FORWARD -i sentinel-data -o sentinel-mgmt -j DROP
      iptables -A FORWARD -i sentinel-data -o sentinel-tool -j DROP
      # Allow established/related
      iptables -A FORWARD -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
      # Default deny
      iptables -P FORWARD DROP
    '';

    # Storage pool setup
    systemd.services.sentinel-storage-setup = {
      description = "Sentinel Compartment Storage Setup";
      wantedBy = [ "multi-user.target" ];
      before = [ "sentinel-crosvm.service" "sentinel-qemu.service" ];
      script = ''
        mkdir -p ${cfg.storage.pool}
        # Create default disk images if they don't exist
        for profile in minimal core full; do
          if [ ! -f "${cfg.storage.pool}/${profile}.img" ]; then
            ${pkgs.qemu-img}/bin/qemu-img create -f qcow2 \\
              "${cfg.storage.pool}/${profile}.img" ${cfg.storage.defaultSize}
          fi
        done
        # Set permissions
        chown -R sentinel:kvm ${cfg.storage.pool}
        chmod 750 ${cfg.storage.pool}
      '';
    };

    # Capability broker service
    systemd.services.sentinel-capability-broker = {
      description = "Sentinel Capability Broker";
      wantedBy = [ "multi-user.target" ];
      after = [ "network-online.target" "sentinel-crosvm.service" "sentinel-qemu.service" ];
      serviceConfig = {
        ExecStart = "${pkgs.writeScriptBin "sentinel-capability-broker" ''}
          #!${pkgs.runtimeShell}
          # Capability broker daemon
          # Validates compartment requests against policy
          # Issues capability tokens for compartment spawn
          exec ${pkgs.python3}/bin/python3 -c "
import json, sys, os, socket, ssl
from pathlib import Path

POLICY_DIR = Path('/etc/sentinel/compartments/policies')
CAPABILITY_STORE = Path('/run/sentinel/capabilities')

class CapabilityBroker:
    def __init__(self):
        self.policies = self.load_policies()

    def load_policies(self):
        policies = {}
        for f in POLICY_DIR.glob('*.json'):
            with open(f) as fp:
                policies[f.stem] = json.load(fp)
        return policies

    def validate_request(self, compartment_type, requested_caps):
        policy = self.policies.get(compartment_type, {})
        allowed = policy.get('capabilities', [])
        for cap in requested_caps:
            if cap not in allowed:
                return False, f'Capability {cap} not allowed for {compartment_type}'
        return True, 'OK'

    def issue_token(self, compartment_type, caps):
        import secrets, time
        token = {
            'compartment': compartment_type,
            'capabilities': caps,
            'issued': int(time.time()),
            'nonce': secrets.token_hex(16),
        }
        # Sign token (in production, use proper signing)
        token['signature'] = secrets.token_hex(32)
        return token

    def run(self):
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.bind('/run/sentinel/capability-broker.sock')
        sock.listen(5)
        os.chmod('/run/sentinel/capability-broker.sock', 0o660)

        while True:
            conn, _ = sock.accept()
            try:
                data = conn.recv(4096)
                req = json.loads(data)
                if req['action'] == 'validate':
                    ok, msg = self.validate_request(req['compartment'], req['capabilities'])
                    conn.send(json.dumps({'ok': ok, 'message': msg}).encode())
                elif req['action'] == 'issue':
                    ok, msg = self.validate_request(req['compartment'], req['capabilities'])
                    if ok:
                        token = self.issue_token(req['compartment'], req['capabilities'])
                        conn.send(json.dumps({'ok': True, 'token': token}).encode())
                    else:
                        conn.send(json.dumps({'ok': False, 'message': msg}).encode())
            finally:
                conn.close()

if __name__ == '__main__':
    broker = CapabilityBroker()
    broker.run()
          ";
          User = "sentinel";
          Group = "sentinel";
          StateDirectory = "sentinel";
          CapabilityBoundingSet = "CAP_DAC_OVERRIDE";
        };
      };
    };

    # Compartment policy directory
    systemd.tmpfiles.rules = [
      "d /etc/sentinel/compartments/policies 0750 sentinel sentinel -"
      "d /run/sentinel/capabilities 0750 sentinel sentinel -"
      "d /run/sentinel/compartments 0750 sentinel sentinel -"
      "d ${cfg.storage.pool} 0750 sentinel kvm -"
    ];

    # Example compartment policies
    systemd.services.sentinel-compartment-policies = {
      description = "Install Default Compartment Policies";
      wantedBy = [ "multi-user.target" ];
      before = [ "sentinel-capability-broker.service" ];
      script = ''
        mkdir -p /etc/sentinel/compartments/policies

        # Tool compartment policy
        cat > /etc/sentinel/compartments/policies/tool.json << 'EOF'
        {
          "name": "tool",
          "description": "Generic security tool compartment",
          "capabilities": [
            "network.raw",
            "filesystem.read:/nix/store",
            "filesystem.write:/data",
            "filesystem.read:/var/lib/sentinel/models",
            "gpu.compute",
            "cpu:2",
            "memory:4G"
          ],
          "network": "tool",
          "seccomp": "default"
        }
        EOF

        # LLM inference compartment
        cat > /etc/sentinel/compartments/policies/llm-inference.json << 'EOF'
        {
          "name": "llm-inference",
          "description": "Local LLM inference compartment",
          "capabilities": [
            "filesystem.read:/var/lib/sentinel/models",
            "filesystem.write:/tmp",
            "gpu.compute",
            "cpu:4",
            "memory:16G",
            "hugepages:2G"
          ],
          "network": "none",
          "seccomp": "restricted"
        }
        EOF

        # Data compartment
        cat > /etc/sentinel/compartments/policies/data.json << 'EOF'
        {
          "name": "data",
          "description": "Data storage compartment (evidence, logs, captures)",
          "capabilities": [
            "filesystem.read:/data",
            "filesystem.write:/data",
            "cpu:1",
            "memory:2G"
          ],
          "network": "data",
          "seccomp": "minimal"
        }
        EOF

        # Operator compartment
        cat > /etc/sentinel/compartments/policies/operator.json << 'EOF'
        {
          "name": "operator",
          "description": "Operator interface compartment",
          "capabilities": [
            "filesystem.read:/nix/store",
            "filesystem.read:/home/sentinel",
            "filesystem.write:/home/sentinel",
            "display.wayland",
            "input.keyboard",
            "input.mouse",
            "network.local",
            "cpu:2",
            "memory:4G"
          ],
          "network": "mgmt",
          "seccomp": "default"
        }
        EOF

        # Threat intel compartment
        cat > /etc/sentinel/compartments/policies/threat-intel.json << 'EOF'
        {
          "name": "threat-intel",
          "description": "Threat intelligence / ML anomaly detection compartment",
          "capabilities": [
            "filesystem.read:/var/lib/sentinel/models",
            "filesystem.read:/data",
            "filesystem.write:/var/lib/sentinel/threat-intel",
            "gpu.compute",
            "cpu:4",
            "memory:8G"
          ],
          "network": "tool",
          "seccomp": "restricted"
        }
        EOF
      '';
    };

    # Compartment CLI tool
    environment.systemPackages = with pkgs; [
      (writeScriptBin "sentinel-compartment" ''
        #!${pkgs.runtimeShell}
        set -euo pipefail

        usage() {
          cat << 'EOF'
    Usage: sentinel-compartment <command> [options]

    Commands:
      create <type> <name>     Create a new compartment
      start <name>             Start a compartment
      stop <name>              Stop a compartment
      destroy <name>           Destroy a compartment
      list                     List all compartments
      exec <name> <cmd>        Execute command in compartment
      logs <name>              Show compartment logs
      policy <type>            Show policy for compartment type

    Types:
      tool         Generic security tool
      llm          LLM inference
      data         Data storage
      operator     Operator interface
      threat-intel Threat intelligence
    EOF
        }

        if [ $# -eq 0 ]; then
          usage
          exit 1
        fi

        cmd="$1"
        shift

        case "$cmd" in
          create)
            type="$1"
            name="$2"
            if [ -z "$type" ] || [ -z "$name" ]; then
              echo "Usage: sentinel-compartment create <type> <name>"
              exit 1
            fi
            echo "Creating $type compartment: $name"
            # Implementation would call capability broker
            ;;
          start)
            name="$1"
            echo "Starting compartment: $name"
            ;;
          stop)
            name="$1"
            echo "Stopping compartment: $name"
            ;;
          destroy)
            name="$1"
            echo "Destroying compartment: $name"
            ;;
          list)
            echo "Listing compartments..."
            ;;
          exec)
            name="$1"
            shift
            echo "Executing in $name: $*"
            ;;
          logs)
            name="$1"
            echo "Showing logs for: $name"
            ;;
          policy)
            type="$1"
            cat "/etc/sentinel/compartments/policies/${type}.json"
            ;;
          *)
            echo "Unknown command: $cmd"
            usage
            exit 1
            ;;
        esac
      '')
    ];

    # AppArmor profiles for compartments
    security.apparmor.profiles = {
      "sentinel-compartment-tool" = ''
        #include <tunables/global>
        profile sentinel-compartment-tool flags=(attach_disconnected) {
          #include <abstractions/base>
          #include <abstractions/nameservice>

          # Network
          network inet raw,
          network inet6 raw,
          network inet stream,
          network inet6 stream,

          # Capabilities
          capability net_raw,
          capability net_admin,
          capability dac_override,
          capability sys_resource,

          # Filesystem
          /nix/store/** r,
          /var/lib/sentinel/models/** r,
          /data/** rw,
          /tmp/** rw,

          # Deny dangerous operations
          deny mount,
          deny umount,
          deny ptrace,
          deny signal,

          # Exec
          /usr/bin/** ix,
          /bin/** ix,
        }
      '';
      "sentinel-compartment-llm" = ''
        #include <tunables/global>
        profile sentinel-compartment-llm flags=(attach_disconnected) {
          #include <abstractions/base>

          # No network
          deny network,

          # Capabilities
          capability sys_resource,

          # Filesystem
          /var/lib/sentinel/models/** r,
          /tmp/** rw,
          /run/sentinel/** rw,

          # GPU (via render nodes)
          /dev/dri/** rw,
          /dev/nvidia* rw,

          # Exec
          /nix/store/*/bin/llama-* ix,
          /nix/store/*/bin/vllm ix,
          /nix/store/*/bin/python* ix,
        }
      '';
      "sentinel-compartment-data" = ''
        #include <tunables/global>
        profile sentinel-compartment-data flags=(attach_disconnected) {
          #include <abstractions/base>

          # Network (data VLAN only)
          network inet stream,
          network inet6 stream,

          # Filesystem
          /data/** rw,
          /var/lib/sentinel/data/** rw,

          # Deny exec
          deny /usr/bin/** x,
          deny /bin/** x,

          # Deny dangerous
          deny mount,
          deny umount,
          deny ptrace,
        }
      '';
    };

    # seL4 integration (future)
    # This would integrate with seL4 microkernel for strongest isolation
    # For now, we use crosvm/QEMU/Kata with capability-based access control
  };
}